# dprint-plugin-svg

dprint Wasm plugin for formatting SVG files with `svg-format`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build -p dprint-plugin-svg --release --target wasm32-unknown-unknown
```

This outputs:

`target/wasm32-unknown-unknown/release/dprint_plugin_svg.wasm`

## Use in dprint

```json
{
	"plugins": ["./target/wasm32-unknown-unknown/release/dprint_plugin_svg.wasm"],
	"svg": {
		"attributeSort": "canonical",
		"attributeLayout": "auto",
		"attributesPerLine": 1,
		"wrappedAttributeIndent": "one-level"
	}
}
```

## Supported SVG Plugin Config

- `lineWidth` (number)
- `maxInlineTagWidth` (number)
- `useTabs` (boolean)
- `indentWidth` (number)
- `newLineKind` (`"auto" | "lf" | "crlf" | "system"`)
- `attributeSort` (`"none" | "canonical" | "alphabetical"`)
- `attributeLayout` (`"auto" | "single-line" | "multi-line"`)
- `attributesPerLine` (number > 0)
- `spaceBeforeSelfClose` (boolean)
- `quoteStyle` (`"preserve" | "double" | "single"`)
- `wrappedAttributeIndent` (`"one-level" | "align-to-tag-name"`)
