interface Props {
	pkg: string;
	param: string;
	label: string;
}

/**
 * Dashed inline input for picking upstream package versions. Emits its
 * own `<script type="module" src="/version-picker.mjs">` so the widget
 * is self-contained. The JS locator is `input.ver[data-pkg][data-param]`,
 * so those classes/attributes MUST stay stable. Multi-instance rendering
 * is free: the module script dedupes by URL and evaluates once.
 */
export function VersionCombo({ pkg, param, label }: Props) {
	return (
		<span class="ver-wrap">
			<input
				type="text"
				class="ver"
				data-pkg={pkg}
				data-param={param}
				placeholder="version"
				autocomplete="off"
				aria-label={label}
			/>
			<ul class="ver-list" hidden></ul>
			<script type="module" src="/version-picker.mjs"></script>
		</span>
	);
}
