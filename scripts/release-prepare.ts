#!/usr/bin/env bun
import { $, file, write } from "bun";
import { normalize, relative } from "node:path";

const version = Bun.argv[2];
if (!version || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
	console.error(`usage: ${relative(process.cwd(), import.meta.filename)} <version>`);
	process.exit(1);
}

const status = (await $`git status --short`.text()).trim();
if (status !== "") {
	console.error("working tree must be clean before preparing a release");
	process.exit(1);
}

const tag = `v${version}`;
const tagRef = `refs/tags/${tag}`;
const tagCheck = await $`git rev-parse --verify ${tagRef}`.nothrow().quiet();
if (tagCheck.exitCode === 0) {
	console.error(`tag ${tag} already exists`);
	process.exit(1);
}

const cargoTomlPath = normalize(`${import.meta.dir}/../Cargo.toml`);
const cargoTomlSource = await file(cargoTomlPath).text();
const cargoTomlParsed = Bun.TOML.parse(cargoTomlSource) as {
	workspace?: { package?: { version?: string } };
};

if (cargoTomlParsed.workspace?.package?.version == null) {
	console.error("Cargo.toml is missing workspace.package.version");
	process.exit(1);
}

const cargoToml = cargoTomlSource.replace(
	/(\[workspace\.package\][\s\S]*?version\s*=\s*")(.*?)("\n)/,
	`$1${version}$3`,
);

if (cargoToml === cargoTomlSource) {
	console.error("failed to update workspace.package.version in Cargo.toml");
	process.exit(1);
}

const updatedCargoToml = Bun.TOML.parse(cargoToml) as {
	workspace?: { package?: { version?: string } };
};

if (updatedCargoToml.workspace?.package?.version !== version) {
	console.error("Cargo.toml version update did not produce the expected workspace.package.version");
	process.exit(1);
}

await write(cargoTomlPath, cargoToml);

await $`cargo check --workspace`;
await $`just ci`;
await $`git add Cargo.toml Cargo.lock`;
await $`git commit -m ${`chore(release): ${tag}`}`;
await $`git tag -a ${tag} -m ${tag}`;

const branch = (await $`git branch --show-current`.text()).trim();
console.log("release prepared locally");
console.log(`next: git push origin ${branch}`);
console.log(`next: git push origin ${tag}`);
