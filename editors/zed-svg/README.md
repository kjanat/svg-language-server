# SVG extension for Zed

> [!IMPORTANT]
> THIS EXTENSION IS NOT PUBLISHED NOR DO I HAVE A TIMELINE FOR PUBLICATION. IT
> IS CURRENTLY IN DEVELOPMENT AND SUBJECT TO BREAKING CHANGES.

<!--
<a href="zed://extension/svg" title="Install in Zed">
  <img alt="Install in Zed" src="https://img.shields.io/badge/Install%20in%20Zed-084CCF?logo=zedindustries&logoColor=white&logoSize=auto&labelColor=084CCF&link=zed%3A%2F%2Fextension%2Fsvg&link=zed%3A%2F%2Fextension%2Fsvg">
</a>
-->

Syntax highlighting for Scalable Vector Graphics (`.svg`) files, backed by the
local [`tree-sitter-svg`] grammar.

## Features

- XML-aware SVG parsing
- Structured SVG path data
- CSS injections for `<style>` and `style="..."`
- JavaScript injections for `<script>` and `on*` handlers
- HTML injections for `<foreignObject>` content

## File associations

- `.svg` - SVG

## Install

[Install] from ~~Zed's extension marketplace~~, or for development:

```text
zed: Install Dev Extension
```

Point Zed to `editors/zed-svg`.

[Install]: zed://extension/svg "Install in Zed"
[`tree-sitter-svg`]: ../../grammars/tree-sitter-svg
