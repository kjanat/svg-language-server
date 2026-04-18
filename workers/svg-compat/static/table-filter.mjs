/**
 * Per-table live filter. Finds every <input data-filter-target="ID">,
 * binds an input listener, and hides <tr data-search> rows in the
 * matching <section id="ID"> whose token string doesn't contain the
 * query. Updates a visible / total counter next to the input.
 *
 * Pressing Escape clears the input and restores the table.
 */

/** @param {HTMLInputElement} input */
function bindFilter(input) {
	const targetId = input.dataset.filterTarget;
	if (!targetId) return;
	const section = document.getElementById(targetId);
	if (!section) return;
	const rows = Array.from(section.querySelectorAll("tbody tr[data-search]"));
	const empty = section.querySelector("tbody tr.table-empty");
	const counter = section.querySelector(`[data-filter-count="${targetId}"]`);
	const total = rows.length;

	/** @param {string} query */
	const apply = (query) => {
		let visible = 0;
		for (const row of rows) {
			const tokens = row.dataset.search ?? "";
			const match = query.length === 0 || tokens.includes(query);
			row.hidden = !match;
			if (match) visible += 1;
		}
		if (empty instanceof HTMLElement) {
			empty.hidden = !(query.length > 0 && visible === 0);
		}
		if (counter) counter.textContent = `${visible} / ${total}`;
	};

	apply("");

	let frame = 0;
	input.addEventListener("input", () => {
		if (frame !== 0) cancelAnimationFrame(frame);
		frame = requestAnimationFrame(() => {
			frame = 0;
			apply(input.value.trim().toLowerCase());
		});
	});

	input.addEventListener("keydown", (event) => {
		if (event.key === "Escape" && input.value.length > 0) {
			event.preventDefault();
			input.value = "";
			apply("");
		}
	});
}

for (const input of document.querySelectorAll("input[data-filter-target]")) {
	if (input instanceof HTMLInputElement) bindFilter(input);
}
