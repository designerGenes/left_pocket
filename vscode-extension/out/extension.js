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
let activePocketDir;
function getSpocketDir() {
    return path.join(os.homedir(), ".safe_pocket");
}
function isSpocketWorkspace(workspaceFile) {
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
function getBinaryPath() {
    const config = vscode.workspace.getConfiguration("spocket");
    const configured = config.get("binaryPath")?.trim();
    if (configured && configured !== "spocket") {
        return configured;
    }
    const installedBinary = path.join(os.homedir(), ".local", "bin", "safe_pocket");
    if (fs.existsSync(installedBinary)) {
        return installedBinary;
    }
    return "safe_pocket";
}
function runSync(pocketDir) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, ["sync", "--pocket", pocketDir], (error, stdout) => {
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
function runMergeCommand(action, pocketDir) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, [action, "--pocket", pocketDir], (error) => {
            if (error) {
                console.error(`spocket ${action} failed: ${error.message}`);
            }
            resolve();
        });
    });
}
function locatePocketDir(workspacePath) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, ["locate", "--path", workspacePath], (error, stdout) => {
            if (error) {
                console.error(`spocket locate failed: ${error.message}`);
                resolve(undefined);
                return;
            }
            try {
                const result = JSON.parse(stdout.trim());
                resolve(result.status === "found" ? result.pocket_dir : undefined);
            }
            catch {
                console.error(`Failed to parse locate output: ${stdout}`);
                resolve(undefined);
            }
        });
    });
}
async function resolveActivePocketDir() {
    if (activePocketDir) {
        return activePocketDir;
    }
    const workspacePocket = isSpocketWorkspace(vscode.workspace.workspaceFile);
    if (workspacePocket) {
        activePocketDir = workspacePocket;
        return workspacePocket;
    }
    for (const folder of vscode.workspace.workspaceFolders ?? []) {
        if (folder.uri.scheme !== "file") {
            continue;
        }
        const located = await locatePocketDir(folder.uri.fsPath);
        if (located) {
            activePocketDir = located;
            return located;
        }
    }
    return undefined;
}
function appendPocketEvent(pocketDir, action, details) {
    const eventPath = path.join(pocketDir, "events.jsonl");
    const event = {
        timestamp: new Date().toISOString(),
        action,
        details,
    };
    fs.appendFile(eventPath, `${JSON.stringify(event)}\n`, (error) => {
        if (error) {
            console.error(`spocket event log failed: ${error.message}`);
        }
    });
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
async function findDatedFeatureFiles(featuresDir) {
    const results = [];
    async function walk(dir) {
        let entries;
        try {
            entries = await vscode.workspace.fs.readDirectory(dir);
        }
        catch {
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
async function openDailyFeature() {
    const pocketDir = await resolveActivePocketDir();
    if (!pocketDir) {
        vscode.window.showWarningMessage("Spocket: no active safe pocket workspace.");
        return;
    }
    const config = vscode.workspace.getConfiguration("spocket");
    const configuredSubpath = config.get("dailyFeatureSubpath")?.trim() ?? "";
    const safeSubpath = configuredSubpath
        .split(/[\\/]+/)
        .filter((part) => part && part !== "." && part !== "..")
        .join("/");
    const featuresDir = vscode.Uri.file(path.join(pocketDir, "FEATURES"));
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
    const existingTarget = candidates[0]?.uri;
    const target = existingTarget ?? vscode.Uri.file(path.join(pocketDir, "FEATURES", safeSubpath, `${today.replace(/-/g, "_")}.md`));
    if (!fs.existsSync(target.fsPath)) {
        await vscode.workspace.fs.createDirectory(vscode.Uri.file(path.dirname(target.fsPath)));
        await vscode.workspace.fs.writeFile(target, Buffer.from(`# ${today}\n\n`, "utf8"));
        appendPocketEvent(pocketDir, "daily_feature.create", { path: target.fsPath });
    }
    else {
        appendPocketEvent(pocketDir, "daily_feature.open", { path: target.fsPath });
    }
    const doc = await vscode.workspace.openTextDocument(target);
    await vscode.window.showTextDocument(doc);
}
async function handleFolderChange(pocketDir) {
    if (syncInProgress) {
        return;
    }
    syncInProgress = true;
    updateStatusBar("$(sync~spin) Spocket syncing...");
    try {
        const result = await runSync(pocketDir);
        switch (result.status) {
            case "unchanged":
                updateStatusBar(`$(check) Spocket`, `Hash: ${result.hash}\nBirth: ${result.birth_hash}`);
                break;
            case "synced": {
                updateStatusBar(`$(check) Spocket`, `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`);
                // Count added/removed for the notification
                const added = (result.paths?.length ?? 0) -
                    ((result.paths?.length ?? 0) -
                        ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));
                vscode.window.setStatusBarMessage(`Spocket: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`, 5000);
                break;
            }
            case "error":
                updateStatusBar("$(warning) Spocket", `Error: ${result.message}`);
                vscode.window.showWarningMessage(`Spocket sync failed: ${result.message}`);
                break;
        }
    }
    catch (err) {
        updateStatusBar("$(error) Spocket", "Sync failed");
    }
    finally {
        syncInProgress = false;
    }
}
async function activate(context) {
    context.subscriptions.push(vscode.commands.registerCommand("spocket.openDailyFeature", openDailyFeature));
    const pocketDir = await resolveActivePocketDir();
    if (!pocketDir) {
        return;
    }
    activePocketDir = pocketDir;
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.text = "$(check) Spocket";
    statusBarItem.tooltip = `Pocket: ${path.basename(pocketDir)}`;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);
    const disposable = vscode.workspace.onDidChangeWorkspaceFolders(() => {
        handleFolderChange(pocketDir);
    });
    context.subscriptions.push(disposable);
    handleFolderChange(pocketDir);
    runMergeCommand("merge-start", pocketDir).catch((err) => console.error("spocket merge-start failed:", err));
}
function deactivate() {
    statusBarItem?.dispose();
    statusBarItem = undefined;
    if (activePocketDir) {
        return runMergeCommand("merge-stop", activePocketDir);
    }
}
//# sourceMappingURL=extension.js.map