#!/usr/bin/env bun
import { error, log } from "node:console";
import { normalize, relative } from "node:path";
import { argv, cwd, exit } from "node:process";

const version = argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
	error(`usage: ${relative(cwd(), import.meta.filename!)} <version>`);
	exit(1);
}

const status = (await Bun.$`git status --short`.text()).trim();
if (status !== "") {
	error("working tree must be clean before preparing a release");
	exit(1);
}

const tag = `v${version}`;
const tagRef = `refs/tags/${tag}`;
const tagCheck = await Bun.$`git rev-parse --verify ${tagRef}`.nothrow().quiet();
if (tagCheck.exitCode === 0) {
	error(`tag ${tag} already exists`);
	exit(1);
}

const cargoTomlPath = normalize(`${import.meta.dir}/../Cargo.toml`);
const cargoTomlSource = await Bun.file(cargoTomlPath).text();
const cargoTomlParsed = Bun.TOML.parse(cargoTomlSource) as {
	workspace?: { package?: { version?: string } };
};

if (cargoTomlParsed.workspace?.package?.version == null) {
	error("Cargo.toml is missing workspace.package.version");
	exit(1);
}

const cargoToml = cargoTomlSource.replace(
	/(\[workspace\.package\][\s\S]*?version\s*=\s*")(.*?)("\n)/,
	`$1${version}$3`,
);

if (cargoToml === cargoTomlSource) {
	error("failed to update workspace.package.version in Cargo.toml");
	exit(1);
}

const updatedCargoToml = Bun.TOML.parse(cargoToml) as {
	workspace?: { package?: { version?: string } };
};

if (updatedCargoToml.workspace?.package?.version !== version) {
	error("Cargo.toml version update did not produce the expected workspace.package.version");
	exit(1);
}

await Bun.write(cargoTomlPath, cargoToml);

await Bun.$`cargo check --workspace`;
await Bun.$`just verify`;
await Bun.$`git add Cargo.toml Cargo.lock`;
await Bun.$`git commit -m ${`chore(release): ${tag}`}`;
await Bun.$`git tag -a ${tag} -m ${tag}`;

const branch = (await Bun.$`git branch --show-current`.text()).trim();
log("release prepared locally");
log(`next: git push origin ${branch}`);
log(`next: git push origin ${tag}`);
