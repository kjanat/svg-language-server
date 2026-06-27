declare module 'node-gyp-build' {
	import type Parser from 'tree-sitter';

	function nodeGypBuild(dir?: string): Parser.Language & Record<string, unknown>;

	export = nodeGypBuild;
}
