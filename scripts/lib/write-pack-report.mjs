#!/usr/bin/env node
// =============================================================================
// write-pack-report.mjs — emit a pack-report.json for a component packed by the
// `arcus` binary directly rather than through pack-arcus.sh.
//
// Usage:
//   node scripts/lib/write-pack-report.mjs \
//     --output-dir DIR --package-id ID --version V --sequence N \
//     --source-id ID --channel C --target-id T --envelope PATH
// =============================================================================

import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

function parseArgs(argv) {
    const out = {};
    for (let i = 0; i < argv.length; i += 1) {
        const arg = argv[i];
        if (!arg.startsWith("--")) {
            console.error(`error: unexpected positional argument: ${arg}`);
            process.exit(1);
        }
        const key = arg.slice(2);
        const value = argv[i + 1];
        if (value === undefined || value.startsWith("--")) {
            console.error(`error: missing value for --${key}`);
            process.exit(1);
        }
        out[key] = value;
        i += 1;
    }
    return out;
}

const REQUIRED = [
    "output-dir",
    "package-id",
    "version",
    "sequence",
    "source-id",
    "channel",
    "target-id",
    "envelope",
];

const args = parseArgs(process.argv.slice(2));
const missing = REQUIRED.filter((k) => !args[k]);
if (missing.length > 0) {
    console.error(`error: missing required option(s): ${missing.map((k) => `--${k}`).join(", ")}`);
    process.exit(1);
}

/** Measure one artifact, or return null when it was not produced. */
function measure(path) {
    if (!existsSync(path)) return null;
    return {
        file: path,
        filename: basename(path),
        sha256: createHash("sha256").update(readFileSync(path)).digest("hex"),
        size_bytes: statSync(path).size,
    };
}

/** Flatten a measurement into the wrapper's <prefix>_file/_filename/... shape. */
function spread(prefix, m) {
    if (!m) return {};
    return {
        [`${prefix}_file`]: m.file,
        [`${prefix}_filename`]: m.filename,
        [`${prefix}_sha256`]: m.sha256,
        [`${prefix}_size_bytes`]: m.size_bytes,
    };
}

const outputDir = args["output-dir"];
const stem = `${args["package-id"]}-${args.version}-${args["target-id"]}`;

const archive = measure(join(outputDir, `${stem}.tar.zst`));
const content = measure(join(outputDir, `${stem}-content.zip`));
const treesig = measure(join(outputDir, `${stem}.pwr`));
const envelope = measure(args.envelope);

if (!archive) {
    console.error(`error: no archive found for ${stem} in ${outputDir}`);
    process.exit(1);
}
if (!envelope) {
    console.error(`error: envelope not found: ${args.envelope}`);
    process.exit(1);
}

const targetEntry = {
    target_id: args["target-id"],
    ...spread("archive", archive),
    ...spread("content_source", content),
    ...spread("tree_signature", treesig),
};

const report = {
    release_id: `${args["package-id"]}-${args.version}-${args.sequence}`,
    version: args.version,
    sequence: Number(args.sequence),
    source_id: args["source-id"],
    package_id: args["package-id"],
    channel: args.channel,
    envelope_path: envelope.file,
    envelope_filename: envelope.filename,
    envelope_sha256: envelope.sha256,
    ...spread("archive", archive),
    ...spread("content_source", content),
    ...spread("tree_signature", treesig),
    target_id: args["target-id"],
    targets: [targetEntry],
    generated_by: "scripts/lib/write-pack-report.mjs",
};

const reportPath = join(outputDir, "pack-report.json");
writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(`pack-report: ${reportPath}`);
