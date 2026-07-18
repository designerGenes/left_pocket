"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const os = __importStar(require("os"));
let statusBarItem;
let syncInProgress = false;
let activeCornerDir;
const CORNER_ROOT_NAMES = [".corner", ".safe_pocket", ".spocket"];
const CONFIG_ROOT_NAMES = ["corner", "safe_pocket", "spocket"];
function getCornerRoots() {
    return CORNER_ROOT_NAMES.map((name) => path.join(os.homedir(), name));
}
function isCornerWorkspace(workspaceFile) {
    if (!workspaceFile) {
        return undefined;
    }
    const filePath = workspaceFile.fsPath;
    for (const cornerRootDir of getCornerRoots()) {
        if (filePath.startsWith(cornerRootDir)) {
            return path.dirname(filePath);
        }
    }
    return undefined;
}
function getBinaryPath() {
    const config = vscode.workspace.getConfiguration("corner");
    const configured = config.get("binaryPath")?.trim();
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
function runSync(cornerDir) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, ["sync", "--corner", cornerDir], (error, stdout) => {
            if (error) {
                resolve({
                    status: "error",
                    message: error.message,
                });
                return;
            }
            try {
                const result = JSON.parse(stdout.trim());
                resolve(result);
            }
            catch {
                resolve({
                    status: "error",
                    message: `Failed to parse sync output: ${stdout}`,
                });
            }
        });
    });
}
function runMergeCommand(action, cornerDir) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, [action, "--corner", cornerDir], (error) => {
            if (error) {
                console.error(`corner ${action} failed: ${error.message}`);
            }
            resolve();
        });
    });
}
function locateCornerDir(workspacePath) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, ["locate", "--path", workspacePath], (error, stdout) => {
            if (error) {
                console.error(`corner locate failed: ${error.message}`);
                resolve(undefined);
                return;
            }
            try {
                const result = JSON.parse(stdout.trim());
                resolve(result.status === "found" ? result.corner_dir : undefined);
            }
            catch {
                console.error(`Failed to parse locate output: ${stdout}`);
                resolve(undefined);
            }
        });
    });
}
async function resolveActiveCornerDir() {
    if (activeCornerDir) {
        return activeCornerDir;
    }
    const workspaceCorner = isCornerWorkspace(vscode.workspace.workspaceFile);
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
function updateStatusBar(text, tooltip) {
    if (statusBarItem) {
        statusBarItem.text = text;
        if (tooltip) {
            statusBarItem.tooltip = tooltip;
        }
    }
}
function pad2(value) {
    return value.toString().padStart(2, "0");
}
function localDateKey(date) {
    return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}
function parseDatedFilename(fileName) {
    const stem = fileName.replace(/\.md$/i, "");
    const currentYear = new Date().getFullYear();
    const monthNames = {
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
function runDailyFeature(cornerDir, isNew) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        const args = ["daily-feature", "--corner", cornerDir];
        if (isNew) {
            args.push("--new");
        }
        const config = vscode.workspace.getConfiguration("corner");
        const subpath = config.get("dailyFeatureSubpath")?.trim();
        if (subpath) {
            args.push("--subpath", subpath);
        }
        (0, child_process_1.execFile)(binary, args, (error, stdout) => {
            if (error) {
                resolve({ status: "error", message: error.message });
                return;
            }
            try {
                const result = JSON.parse(stdout.trim());
                resolve(result);
            }
            catch {
                resolve({
                    status: "error",
                    message: `Failed to parse daily-feature output: ${stdout}`,
                });
            }
        });
    });
}
function featureTagsYamlPath() {
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
function isTodaysFeatureFile(cornerDir, fsPath) {
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
async function openPath(target) {
    const uri = vscode.Uri.file(target);
    const doc = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(doc);
}
async function openFeatureTagsYaml() {
    const tagsPath = featureTagsYamlPath();
    if (!fs.existsSync(tagsPath)) {
        await vscode.workspace.fs.createDirectory(vscode.Uri.file(path.dirname(tagsPath)));
        await vscode.workspace.fs.writeFile(vscode.Uri.file(tagsPath), Buffer.from("", "utf8"));
    }
    const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(tagsPath));
    await vscode.window.showTextDocument(doc, { preview: false });
}
/**
 * Primary hotkey handler.
 *
 * - When the active editor is today's feature file, open feature_tags.yaml.
 * - Otherwise, open (or create) today's daily feature file.
 */
async function openDailyFeature() {
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
        vscode.window.showWarningMessage(`Corner: failed to open daily feature: ${result.message ?? "unknown error"}`);
        return;
    }
    await openPath(result.path);
    await updateFeatureContext();
}
/**
 * Shift+hotkey handler: always create and open a new numbered daily feature
 * file for today.
 */
async function newDailyFeature() {
    const cornerDir = await resolveActiveCornerDir();
    if (!cornerDir) {
        vscode.window.showWarningMessage("Corner: no active corner workspace.");
        return;
    }
    const result = await runDailyFeature(cornerDir, true);
    if (result.status !== "ok" || !result.path) {
        vscode.window.showWarningMessage(`Corner: failed to create daily feature: ${result.message ?? "unknown error"}`);
        return;
    }
    await openPath(result.path);
    await updateFeatureContext();
}
/**
 * Keep the `corner.inTodaysFeature` context key in sync with the active editor
 * so the shift+hotkey binding can be gated to today's feature file.
 */
async function updateFeatureContext() {
    const cornerDir = activeCornerDir ?? (await resolveActiveCornerDir());
    const activePath = vscode.window.activeTextEditor?.document.uri.fsPath;
    const inTodaysFeature = cornerDir
        ? isTodaysFeatureFile(cornerDir, activePath)
        : false;
    await vscode.commands.executeCommand("setContext", "corner.inTodaysFeature", inTodaysFeature);
}
async function handleFolderChange(cornerDir) {
    if (syncInProgress) {
        return;
    }
    syncInProgress = true;
    updateStatusBar("$(sync~spin) Corner syncing...");
    try {
        const result = await runSync(cornerDir);
        switch (result.status) {
            case "unchanged":
                updateStatusBar(`$(check) Corner`, `Hash: ${result.hash}\nBirth: ${result.birth_hash}`);
                break;
            case "synced": {
                updateStatusBar(`$(check) Corner`, `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`);
                // Count added/removed for the notification
                const added = (result.paths?.length ?? 0) -
                    ((result.paths?.length ?? 0) -
                        ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));
                vscode.window.setStatusBarMessage(`Corner: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`, 5000);
                break;
            }
            case "error":
                updateStatusBar("$(warning) Corner", `Error: ${result.message}`);
                vscode.window.showWarningMessage(`Corner sync failed: ${result.message}`);
                break;
        }
    }
    catch (err) {
        updateStatusBar("$(error) Corner", "Sync failed");
    }
    finally {
        syncInProgress = false;
    }
}
async function activate(context) {
    context.subscriptions.push(vscode.commands.registerCommand("corner.openDailyFeature", openDailyFeature), vscode.commands.registerCommand("corner.newDailyFeature", newDailyFeature), vscode.window.onDidChangeActiveTextEditor(() => {
        updateFeatureContext().catch((err) => console.error("corner updateFeatureContext failed:", err));
    }));
    const cornerDir = await resolveActiveCornerDir();
    if (!cornerDir) {
        return;
    }
    activeCornerDir = cornerDir;
    await updateFeatureContext();
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.text = "$(check) Corner";
    statusBarItem.tooltip = `Corner: ${path.basename(cornerDir)}`;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);
    const disposable = vscode.workspace.onDidChangeWorkspaceFolders(() => {
        handleFolderChange(cornerDir);
    });
    context.subscriptions.push(disposable);
    handleFolderChange(cornerDir);
    runMergeCommand("runtime-merge-start", cornerDir).catch((err) => console.error("corner runtime-merge-start failed:", err));
}
function deactivate() {
    statusBarItem?.dispose();
    statusBarItem = undefined;
    if (activeCornerDir) {
        return runMergeCommand("runtime-merge-stop", activeCornerDir);
    }
}
//# sourceMappingURL=extension.js.map