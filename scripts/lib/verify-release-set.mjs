#!/usr/bin/env node
// =============================================================================
// verify-release-set.mjs — fail-closed audit of dist/<version>/<sequence>/
//
// Usage:
//   node scripts/lib/verify-release-set.mjs --root dist/<version>/<sequence> \
//     [--only <component>]
// =============================================================================

import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";

const COMPONENTS = {
    "ck-subc": { kind: "native" },
    "ck": { kind: "native" },
    "ck-subc-mcp": { kind: "native" },
    "ck-uc-discussions": { kind: "native" },
};

const FORBIDDEN_ENTRIES = ["payload", "node_modules"];
const SWEEPABLE_ENTRIES = [".DS_Store", "Thumbs.db"];

function parseArgs(argv) {
    const out = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (!arg.startsWith("--")) {
            console.error(`error: unexpected positional argument: ${arg}`);
            process.exit(2);
        }
        const key = arg.slice(2);
        const value = argv[i + 1];
        if (value === undefined || value.startsWith("--")) {
            console.error(`error: missing value for --${key}`);
            process.exit(2);
        }
        out[key] = value;
        i += 1;
    }
    return out;
}

const args = parseArgs(process.argv.slice(2));
if (!args.root) {
    console.error("error: --root <dist/<version>/<sequence>> is required");
    process.exit(2);
}

const root = args.root;
const only = args.only ?? null;

if (only && !(only in COMPONENTS)) {
    console.error(`error: --only ${only} is not a known component`);
    process.exit(2);
}

if (!existsSync(root)) {
    console.error(`error: release root does not exist: ${root}`);
    process.exit(1);
}

const failures = [];
const checked = [];
const swept = [];

for (const [component, policy] of Object.entries(COMPONENTS)) {
    if (only && only !== component) continue;

    const dir = join(root, component);
    if (!existsSync(dir)) {
        failures.push(`${component}: component directory missing`);
        continue;
    }

    checked.push(component);
    const entries = readdirSync(dir);

    // --- envelope -----------------------------------------------------------
    const releasesDir = join(dir, "releases");
    const envelopes = existsSync(releasesDir)
        ? readdirSync(releasesDir).filter(
              (f) => f.endsWith(".json") && !f.includes("index-policy"),
          )
        : [];
    if (envelopes.length === 0) {
        failures.push(`${component}: no release envelope under releases/`);
    }

    // --- pack report --------------------------------------------------------
    if (!entries.includes("pack-report.json")) {
        failures.push(`${component}: pack-report.json missing`);
    }

    // --- target coverage ----------------------------------------------------
    const archives = entries.filter((f) => f.endsWith(".tar.zst") || f.endsWith(".tar.gz"));
    if (archives.length === 0) {
        failures.push(`${component}: no payload archives`);
    }

    // --- per-archive companion artifacts ------------------------------------
    for (const archive of archives) {
        const stem = archive.replace(/\.tar\.(zst|gz)$/, "");
        for (const [suffix, label] of [
            [`${stem}-content.zip`, "content zip"],
            [`${stem}.pwr`, "tree signature"],
        ]) {
            if (!entries.includes(suffix)) {
                failures.push(`${component}: ${label} missing for ${archive}`);
            }
        }
    }

    // --- staging litter (hard failure) --------------------------------------
    for (const forbidden of FORBIDDEN_ENTRIES) {
        if (entries.includes(forbidden)) {
            failures.push(`${component}: forbidden staging entry present: ${forbidden}`);
        }
    }

    // --- OS litter (swept) --------------------------------------------------
    for (const litter of SWEEPABLE_ENTRIES) {
        const p = join(dir, litter);
        if (existsSync(p)) {
            rmSync(p, { recursive: true, force: true });
            swept.push(`${component}/${litter}`);
        }
    }
}

if (swept.length > 0) {
    console.log(`verify-release-set: swept OS litter: ${swept.join(", ")}`);
}

if (failures.length > 0) {
    console.error("verify-release-set: FAILED:");
    for (const f of failures) {
        console.error(`  • ${f}`);
    }
    process.exit(1);
}

console.log(`verify-release-set: OK (${checked.join(", ")})`);
