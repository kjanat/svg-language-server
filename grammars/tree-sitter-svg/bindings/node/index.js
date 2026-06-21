import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../..', import.meta.url));
const require = createRequire(import.meta.url);
const nodeGypBuild = require('node-gyp-build');

function loadBunBinding() {
	const prebuild = `${root}/prebuilds/${process.platform}-${process.arch}/tree-sitter-svg.node`;

	try {
		return require(prebuild);
	} catch {
		return nodeGypBuild(root);
	}
}

const binding = typeof process.versions.bun === 'string'
	? loadBunBinding()
	: nodeGypBuild(root);

try {
	const nodeTypes = await import(`${root}/src/node-types.json`, { with: { type: 'json' } });
	binding.nodeTypeInfo = nodeTypes.default;
} catch {}

const queries = [
	['HIGHLIGHTS_QUERY', `${root}/queries/highlights.scm`],
	['INJECTIONS_QUERY', `${root}/queries/injections.scm`],
	['LOCALS_QUERY', `${root}/queries/locals.scm`],
	['TAGS_QUERY', `${root}/queries/tags.scm`],
];

for (const [prop, path] of queries) {
	Object.defineProperty(binding, prop, {
		configurable: true,
		enumerable: true,
		get() {
			delete binding[prop];
			try {
				binding[prop] = readFileSync(path, 'utf8');
			} catch {}
			return binding[prop];
		},
	});
}

export default binding;
