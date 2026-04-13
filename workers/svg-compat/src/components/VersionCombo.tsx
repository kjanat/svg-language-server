interface Props {
	pkg: string;
	param: string;
	label: string;
}

/**
 * The dashed inline input that version-picker.mjs binds to.
 * The JS locator is `input.ver[data-pkg][data-param]`, so those
 * classes/attributes MUST stay stable.
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
		</span>
	);
}
