import * as vscode from "vscode";
import { execFile } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";

let statusBarItem: vscode.StatusBarItem | undefined;
let syncInProgress = false;
let activeCornerDir: string | undefined;

const CORNER_ROOT_NAMES = [".corner", ".safe_pocket", ".spocket"];
const CONFIG_ROOT_NAMES = ["corner", "safe_pocket", "spocket"];

interface SyncResult {
  status: "unchanged" | "synced" | "error";
  hash?: string;
  old_hash?: string;
  new_hash?: string;
  birth_hash?: string;
  paths?: string[];
  message?: string;
}

interface LocateResult {
  status: "found" | "not_found";
  corner_dir?: string;
  message?: string;
}

function getCornerRoots(): string[] {
  return CORNER_ROOT_NAMES.map((name) => path.join(os.homedir(), name));
}

function isSpocketWorkspace(
  workspaceFile: vscode.Uri | undefined
): string | undefined {
  if (!workspaceFile) {
    return undefined;
  }

  const filePath = workspaceFile.fsPath;
  for (const spocketDir of getCornerRoots()) {
    if (filePath.startsWith(spocketDir)) {
      return path.dirname(filePath);
    }
  }

  return undefined;
}

function getBinaryPath(): string {
  const config = vscode.workspace.getConfiguration("spocket");
  const configured = config.get<string>("binaryPath")?.trim();

  if (configured && configured !== "corner") {
    return configured;
  }

  for (const binaryName of ["corner", "safe_pocket", "spocket"]) {
    const installedBinary = path.join(os.homedir(), ".local", "bin", binaryName);
    if (fs.existsSync(installedBinary)) {
      return installedBinary;
    }
  }

  return "corner";
}

function runSync(cornerDir: string): Promise<SyncResult> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();

    execFile(binary, ["sync", "--corner", cornerDir], (error, stdout) => {
      if (error) {
        resolve({
          status: "error",
          message: error.message,
        });
        return;
      }

      try {
        const result: SyncResult = JSON.parse(stdout.trim());
        resolve(result);
      } catch {
        resolve({
          status: "error",
          message: `Failed to parse sync output: ${stdout}`,
        });
      }
    });
  });
}

function runMergeCommand(
  action: "runtime-merge-start" | "runtime-merge-stop",
  cornerDir: string
): Promise<void> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();
    execFile(binary, [action, "--corner", cornerDir], (error) => {
      if (error) {
        console.error(`spocket ${action} failed: ${error.message}`);
      }
      resolve();
    });
  });
}

function locateCornerDir(workspacePath: string): Promise<string | undefined> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();

    execFile(binary, ["locate", "--path", workspacePath], (error, stdout) => {
      if (error) {
        console.error(`spocket locate failed: ${error.message}`);
        resolve(undefined);
        return;
      }

      try {
        const result: LocateResult = JSON.parse(stdout.trim());
        resolve(result.status === "found" ? result.corner_dir : undefined);
      } catch {
        console.error(`Failed to parse locate output: ${stdout}`);
        resolve(undefined);
      }
    });
  });
}

async function resolveActiveCornerDir(): Promise<string | undefined> {
  if (activeCornerDir) {
    return activeCornerDir;
  }

  const workspaceCorner = isSpocketWorkspace(vscode.workspace.workspaceFile);
  if (workspaceCorner) {
    activeCornerDir = workspaceCorner;
    return workspaceCorner;
  }

  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    if (folder.uri.scheme !== "file") {
      continue;
    }

    const located = await locateCornerDir(folder.uri.fsPath);
    if (located) {
      activeCornerDir = located;
      return located;
    }
  }

  return undefined;
}

function updateStatusBar(text: string, tooltip?: string): void {
  if (statusBarItem) {
    statusBarItem.text = text;
    if (tooltip) {
      statusBarItem.tooltip = tooltip;
    }
  }
}

function pad2(value: number): string {
  return value.toString().padStart(2, "0");
}

function localDateKey(date: Date): string {
  return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

function parseDatedFilename(fileName: string): string | undefined {
  const stem = fileName.replace(/\.md$/i, "");
  const currentYear = new Date().getFullYear();
  const monthNames: Record<string, number> = {
    january: 1,
    jan: 1,
    february: 2,
    feb: 2,
    march: 3,
    mar: 3,
    april: 4,
    apr: 4,
    may: 5,
    june: 6,
    jun: 6,
    july: 7,
    jul: 7,
    august: 8,
    aug: 8,
    september: 9,
    sep: 9,
    sept: 9,
    october: 10,
    oct: 10,
    november: 11,
    nov: 11,
    december: 12,
    dec: 12,
  };

  let match = stem.match(/^(\d{4})[-_](\d{1,2})[-_](\d{1,2})(?:\D.*)?$/);
  if (match) {
    return `${match[1]}-${pad2(Number(match[2]))}-${pad2(Number(match[3]))}`;
  }

  match = stem.match(/^(\d{1,2})[-_](\d{1,2})[-_](\d{2})(?:\D.*)?$/);
  if (match) {
    return `20${match[3]}-${pad2(Number(match[1]))}-${pad2(Number(match[2]))}`;
  }

  match = stem.match(/^([A-Za-z]+)[-_](\d{1,2})(?:[-_](\d{4}))?(?:\D.*)?$/);
  if (match && monthNames[match[1].toLowerCase()]) {
    return `${match[3] ?? currentYear}-${pad2(monthNames[match[1].toLowerCase()])}-${pad2(Number(match[2]))}`;
  }

  match = stem.match(/^(\d{1,2})[-_]([A-Za-z]+)(?:[-_](\d{4}))?(?:\D.*)?$/);
  if (match && monthNames[match[2].toLowerCase()]) {
    return `${match[3] ?? currentYear}-${pad2(monthNames[match[2].toLowerCase()])}-${pad2(Number(match[1]))}`;
  }

  return undefined;
}

interface DailyFeatureResult {
  status: "ok" | "error";
  path?: string;
  created?: boolean;
  message?: string;
}

function runDailyFeature(
  cornerDir: string,
  isNew: boolean
): Promise<DailyFeatureResult> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();
    const args = ["daily-feature", "--corner", cornerDir];
    if (isNew) {
      args.push("--new");
    }

    const config = vscode.workspace.getConfiguration("spocket");
    const subpath = config.get<string>("dailyFeatureSubpath")?.trim();
    if (subpath) {
      args.push("--subpath", subpath);
    }

    execFile(binary, args, (error, stdout) => {
      if (error) {
        resolve({ status: "error", message: error.message });
        return;
      }

      try {
        const result: DailyFeatureResult = JSON.parse(stdout.trim());
        resolve(result);
      } catch {
        resolve({
          status: "error",
          message: `Failed to parse daily-feature output: ${stdout}`,
        });
      }
    });
  });
}

function featureTagsYamlPath(): string {
  for (const configRoot of CONFIG_ROOT_NAMES) {
    const candidate = path.join(os.homedir(), ".config", configRoot, "feature_tags.yaml");
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return path.join(os.homedir(), ".config", "corner", "feature_tags.yaml");
}

/**
 * Determine whether `fsPath` is "today's" feature file: a markdown file living
 * under the corner's FEATURES directory whose name parses to today's date.
 */
function isTodaysFeatureFile(
  cornerDir: string,
  fsPath: string | undefined
): boolean {
  if (!fsPath) {
    return false;
  }

  const featuresDir = path.join(cornerDir, "FEATURES");
  if (fsPath !== featuresDir && !fsPath.startsWith(featuresDir + path.sep)) {
    return false;
  }

  const name = path.basename(fsPath);
  if (!name.toLowerCase().endsWith(".md")) {
    return false;
  }

  return parseDatedFilename(name) === localDateKey(new Date());
}

async function openPath(target: string): Promise<void> {
  const uri = vscode.Uri.file(target);
  const doc = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(doc);
}

async function openFeatureTagsYaml(): Promise<void> {
  const tagsPath = featureTagsYamlPath();

  if (!fs.existsSync(tagsPath)) {
    await vscode.workspace.fs.createDirectory(
      vscode.Uri.file(path.dirname(tagsPath))
    );
    await vscode.workspace.fs.writeFile(
      vscode.Uri.file(tagsPath),
      Buffer.from("", "utf8")
    );
  }

  const doc = await vscode.workspace.openTextDocument(
    vscode.Uri.file(tagsPath)
  );
  await vscode.window.showTextDocument(doc, { preview: false });
}

/**
 * Primary hotkey handler.
 *
 * - When the active editor is today's feature file, open feature_tags.yaml.
 * - Otherwise, open (or create) today's daily feature file.
 */
async function openDailyFeature(): Promise<void> {
  const cornerDir = await resolveActiveCornerDir();

  if (!cornerDir) {
    vscode.window.showWarningMessage("Corner: no active corner workspace.");
    return;
  }

  const activePath = vscode.window.activeTextEditor?.document.uri.fsPath;
  if (isTodaysFeatureFile(cornerDir, activePath)) {
    await openFeatureTagsYaml();
    return;
  }

  const result = await runDailyFeature(cornerDir, false);
  if (result.status !== "ok" || !result.path) {
    vscode.window.showWarningMessage(
      `Corner: failed to open daily feature: ${result.message ?? "unknown error"}`
    );
    return;
  }

  await openPath(result.path);
  await updateFeatureContext();
}

/**
 * Shift+hotkey handler: always create and open a new numbered daily feature
 * file for today.
 */
async function newDailyFeature(): Promise<void> {
  const cornerDir = await resolveActiveCornerDir();

  if (!cornerDir) {
    vscode.window.showWarningMessage("Corner: no active corner workspace.");
    return;
  }

  const result = await runDailyFeature(cornerDir, true);
  if (result.status !== "ok" || !result.path) {
    vscode.window.showWarningMessage(
      `Corner: failed to create daily feature: ${result.message ?? "unknown error"}`
    );
    return;
  }

  await openPath(result.path);
  await updateFeatureContext();
}

/**
 * Keep the `corner.inTodaysFeature` context key in sync with the active editor
 * so the shift+hotkey binding can be gated to today's feature file.
 */
async function updateFeatureContext(): Promise<void> {
  const cornerDir = activeCornerDir ?? (await resolveActiveCornerDir());
  const activePath = vscode.window.activeTextEditor?.document.uri.fsPath;
  const inTodaysFeature = cornerDir
    ? isTodaysFeatureFile(cornerDir, activePath)
    : false;

  await vscode.commands.executeCommand(
    "setContext",
    "corner.inTodaysFeature",
    inTodaysFeature
  );
}

async function handleFolderChange(cornerDir: string): Promise<void> {
  if (syncInProgress) {
    return;
  }

  syncInProgress = true;
  updateStatusBar("$(sync~spin) Corner syncing...");

  try {
    const result = await runSync(cornerDir);

    switch (result.status) {
      case "unchanged":
        updateStatusBar(
          `$(check) Corner`,
          `Hash: ${result.hash}\nBirth: ${result.birth_hash}`
        );
        break;

      case "synced": {
        updateStatusBar(
          `$(check) Corner`,
          `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`
        );

        // Count added/removed for the notification
        const added =
          (result.paths?.length ?? 0) -
          ((result.paths?.length ?? 0) -
            ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));

        vscode.window.setStatusBarMessage(
          `Corner: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`,
          5000
        );
        break;
      }

      case "error":
        updateStatusBar("$(warning) Corner", `Error: ${result.message}`);
        vscode.window.showWarningMessage(
          `Corner sync failed: ${result.message}`
        );
        break;
    }
  } catch (err) {
    updateStatusBar("$(error) Corner", "Sync failed");
  } finally {
    syncInProgress = false;
  }
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("corner.openDailyFeature", openDailyFeature),
    vscode.commands.registerCommand("corner.newDailyFeature", newDailyFeature),
    vscode.window.onDidChangeActiveTextEditor(() => {
      updateFeatureContext().catch((err) =>
        console.error("spocket updateFeatureContext failed:", err)
      );
    })
  );

  const cornerDir = await resolveActiveCornerDir();

  if (!cornerDir) {
    return;
  }

  activeCornerDir = cornerDir;

  await updateFeatureContext();

  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100
  );
  statusBarItem.text = "$(check) Corner";
  statusBarItem.tooltip = `Corner: ${path.basename(cornerDir)}`;
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  const disposable = vscode.workspace.onDidChangeWorkspaceFolders(() => {
    handleFolderChange(cornerDir);
  });
  context.subscriptions.push(disposable);

  handleFolderChange(cornerDir);

  runMergeCommand("runtime-merge-start", cornerDir).catch((err) =>
    console.error("spocket runtime-merge-start failed:", err)
  );
}

export function deactivate(): Thenable<void> | void {
  statusBarItem?.dispose();
  statusBarItem = undefined;

  if (activeCornerDir) {
    return runMergeCommand("runtime-merge-stop", activeCornerDir);
  }
}
