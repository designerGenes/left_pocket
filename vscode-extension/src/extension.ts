import * as vscode from "vscode";
import { execFile } from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as os from "os";

let statusBarItem: vscode.StatusBarItem | undefined;
let syncInProgress = false;
let activePocketDir: string | undefined;

interface DatedFeatureCandidate {
  uri: vscode.Uri;
  dateKey: string;
  mtimeMs: number;
  ctimeMs: number;
}

interface SyncResult {
  status: "unchanged" | "synced" | "error";
  hash?: string;
  old_hash?: string;
  new_hash?: string;
  birth_hash?: string;
  paths?: string[];
  message?: string;
}

function getSpocketDir(): string {
  return path.join(os.homedir(), ".safe_pocket");
}

function isSpocketWorkspace(
  workspaceFile: vscode.Uri | undefined
): string | undefined {
  if (!workspaceFile) {
    return undefined;
  }

  const filePath = workspaceFile.fsPath;
  const spocketDir = getSpocketDir();

  if (!filePath.startsWith(spocketDir)) {
    return undefined;
  }

  // The pocket dir is the parent of the workspace file
  return path.dirname(filePath);
}

function getBinaryPath(): string {
  const config = vscode.workspace.getConfiguration("spocket");
  const configured = config.get<string>("binaryPath")?.trim();

  if (configured && configured !== "spocket") {
    return configured;
  }

  const installedBinary = path.join(os.homedir(), ".local", "bin", "safe_pocket");

  if (fs.existsSync(installedBinary)) {
    return installedBinary;
  }

  return "safe_pocket";
}

function runSync(pocketDir: string): Promise<SyncResult> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();

    execFile(binary, ["sync", "--pocket", pocketDir], (error, stdout) => {
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
  action: "merge-start" | "merge-stop",
  pocketDir: string
): Promise<void> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();
    execFile(binary, [action, "--pocket", pocketDir], (error) => {
      if (error) {
        console.error(`spocket ${action} failed: ${error.message}`);
      }
      resolve();
    });
  });
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

async function findDatedFeatureFiles(featuresDir: vscode.Uri): Promise<DatedFeatureCandidate[]> {
  const results: DatedFeatureCandidate[] = [];

  async function walk(dir: vscode.Uri): Promise<void> {
    let entries: [string, vscode.FileType][];
    try {
      entries = await vscode.workspace.fs.readDirectory(dir);
    } catch {
      return;
    }

    for (const [name, type] of entries) {
      const uri = vscode.Uri.joinPath(dir, name);
      if (type === vscode.FileType.Directory) {
        await walk(uri);
        continue;
      }
      if (type !== vscode.FileType.File || !name.toLowerCase().endsWith(".md")) {
        continue;
      }

      const dateKey = parseDatedFilename(name);
      if (!dateKey) {
        continue;
      }
      const stat = await vscode.workspace.fs.stat(uri);
      results.push({ uri, dateKey, mtimeMs: stat.mtime, ctimeMs: stat.ctime });
    }
  }

  await walk(featuresDir);
  return results;
}

async function openDailyFeature(): Promise<void> {
  if (!activePocketDir) {
    vscode.window.showWarningMessage("Spocket: no active safe pocket workspace.");
    return;
  }

  const config = vscode.workspace.getConfiguration("spocket");
  const configuredSubpath = config.get<string>("dailyFeatureSubpath")?.trim() ?? "";
  const safeSubpath = configuredSubpath
    .split(/[\\/]+/)
    .filter((part) => part && part !== "." && part !== "..")
    .join("/");
  const featuresDir = vscode.Uri.file(path.join(activePocketDir, "FEATURES"));
  const today = localDateKey(new Date());
  const candidates = (await findDatedFeatureFiles(featuresDir))
    .filter((candidate) => candidate.dateKey === today)
    .sort((a, b) => {
      if (b.mtimeMs !== a.mtimeMs) {
        return b.mtimeMs - a.mtimeMs;
      }
      if (b.ctimeMs !== a.ctimeMs) {
        return b.ctimeMs - a.ctimeMs;
      }
      return path.basename(a.uri.fsPath).localeCompare(path.basename(b.uri.fsPath));
    });

  const target = candidates[0]?.uri ?? vscode.Uri.file(
    path.join(activePocketDir, "FEATURES", safeSubpath, `${today.replace(/-/g, "_")}.md`)
  );

  if (!fs.existsSync(target.fsPath)) {
    await vscode.workspace.fs.createDirectory(vscode.Uri.file(path.dirname(target.fsPath)));
    await vscode.workspace.fs.writeFile(target, Buffer.from(`# ${today}\n\n`, "utf8"));
  }

  const doc = await vscode.workspace.openTextDocument(target);
  await vscode.window.showTextDocument(doc);
}

async function handleFolderChange(pocketDir: string): Promise<void> {
  if (syncInProgress) {
    return;
  }

  syncInProgress = true;
  updateStatusBar("$(sync~spin) Spocket syncing...");

  try {
    const result = await runSync(pocketDir);

    switch (result.status) {
      case "unchanged":
        updateStatusBar(
          `$(check) Spocket`,
          `Hash: ${result.hash}\nBirth: ${result.birth_hash}`
        );
        break;

      case "synced": {
        updateStatusBar(
          `$(check) Spocket`,
          `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`
        );

        // Count added/removed for the notification
        const added =
          (result.paths?.length ?? 0) -
          ((result.paths?.length ?? 0) -
            ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));

        vscode.window.setStatusBarMessage(
          `Spocket: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`,
          5000
        );
        break;
      }

      case "error":
        updateStatusBar("$(warning) Spocket", `Error: ${result.message}`);
        vscode.window.showWarningMessage(
          `Spocket sync failed: ${result.message}`
        );
        break;
    }
  } catch (err) {
    updateStatusBar("$(error) Spocket", "Sync failed");
  } finally {
    syncInProgress = false;
  }
}

export function activate(context: vscode.ExtensionContext): void {
  const pocketDir = isSpocketWorkspace(vscode.workspace.workspaceFile);

  if (!pocketDir) {
    return;
  }

  activePocketDir = pocketDir;

  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100
  );
  statusBarItem.text = "$(check) Spocket";
  statusBarItem.tooltip = `Pocket: ${path.basename(pocketDir)}`;
  statusBarItem.show();
  context.subscriptions.push(statusBarItem);

  const disposable = vscode.workspace.onDidChangeWorkspaceFolders(() => {
    handleFolderChange(pocketDir);
  });
  context.subscriptions.push(disposable);

  context.subscriptions.push(
    vscode.commands.registerCommand("spocket.openDailyFeature", openDailyFeature)
  );

  handleFolderChange(pocketDir);

  runMergeCommand("merge-start", pocketDir).catch((err) =>
    console.error("spocket merge-start failed:", err)
  );
}

export function deactivate(): Thenable<void> | void {
  statusBarItem?.dispose();
  statusBarItem = undefined;

  if (activePocketDir) {
    return runMergeCommand("merge-stop", activePocketDir);
  }
}
