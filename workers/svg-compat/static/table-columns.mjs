/**
 * Column interaction layer for dashboard tables.
 *
 * Features:
 * - Drag header to reorder columns
 * - Drag resize handle to resize columns
 *
 * Applied to all tables inside <section data-searchable>.
 */

const DEFAULT_MIN_COL_WIDTH = 96;
const BROWSER_CHIPS_MODE_TWO = "browser-chips--two";
const BROWSER_CHIPS_MODE_FOUR = "browser-chips--four";

/** @param {string} value */
function parsePixels(value) {
	const parsed = Number.parseFloat(value);
	return Number.isFinite(parsed) ? parsed : 0;
}

/**
 * Calculate how much inline space all chips need on a single row.
 * We use this to force either 4-in-a-row or 2x2, never 3+1.
 *
 * @param {HTMLElement} list
 */
function preferredSingleRowWidth(list) {
	const chips = Array.from(list.children).filter((node) => node instanceof HTMLElement);
	if (chips.length === 0) return 0;

	const style = getComputedStyle(list);
	const gap = parsePixels(style.columnGap || style.gap);
	const contentWidth = chips.reduce((sum, chip) => sum + chip.getBoundingClientRect().width, 0);
	return Math.ceil(contentWidth + gap * Math.max(0, chips.length - 1));
}

/**
 * Toggle support-chip layout so rows are always either 4 or 2.
 *
 * @param {HTMLElement} list
 */
function syncBrowserChipLayout(list) {
	const cell = list.parentElement;
	if (!(cell instanceof HTMLElement)) return;

	const preferredFourWidth = preferredSingleRowWidth(list);
	if (preferredFourWidth <= 0) return;

	const available = cell.getBoundingClientRect().width;
	const canFitFour = available >= preferredFourWidth;
	list.classList.toggle(BROWSER_CHIPS_MODE_FOUR, canFitFour);
	list.classList.toggle(BROWSER_CHIPS_MODE_TWO, !canFitFour);
}

/**
 * @param {ParentNode} root
 */
function syncBrowserChipLayouts(root) {
	for (const node of root.querySelectorAll(".browser-chips")) {
		if (node instanceof HTMLElement) syncBrowserChipLayout(node);
	}
}

/** @param {HTMLTableElement} table */
function ensureColgroup(table) {
	const headerRow = table.tHead?.rows?.[0];
	if (!headerRow) return null;
	const columnCount = headerRow.cells.length;
	if (columnCount === 0) return null;

	let colgroup = table.querySelector("colgroup");
	if (!colgroup) {
		colgroup = document.createElement("colgroup");
		table.insertBefore(colgroup, table.firstChild);
	}

	while (colgroup.children.length < columnCount) {
		colgroup.appendChild(document.createElement("col"));
	}
	while (colgroup.children.length > columnCount) {
		colgroup.lastElementChild?.remove();
	}

	return /** @type {HTMLTableColElement[]} */ (Array.from(colgroup.children));
}

/** @param {HTMLTableElement} table */
function columnCount(table) {
	return table.tHead?.rows?.[0]?.cells.length ?? 0;
}

/**
 * Moves a visual column from one index to another.
 * Rows with colspan/rowspan shape mismatches are skipped.
 *
 * @param {HTMLTableElement} table
 * @param {number} from
 * @param {number} to
 */
function moveColumn(table, from, to) {
	if (from === to) return;
	const count = columnCount(table);
	if (from < 0 || to < 0 || from >= count || to >= count) return;

	for (const row of Array.from(table.rows)) {
		const cells = Array.from(row.cells);
		if (cells.length !== count) continue;
		const moving = cells[from];
		const ref = to > from ? cells[to].nextSibling : cells[to];
		row.insertBefore(moving, ref);
	}

	const colgroup = table.querySelector("colgroup");
	if (!colgroup) return;
	const cols = Array.from(colgroup.children);
	if (cols.length !== count) return;
	const movingCol = cols[from];
	const colRef = to > from ? cols[to].nextSibling : cols[to];
	colgroup.insertBefore(movingCol, colRef);
}

/**
 * @param {HTMLTableElement} table
 * @param {HTMLTableCellElement} th
 */
function getColumnIndex(table, th) {
	const headers = Array.from(table.tHead?.rows?.[0]?.cells ?? []);
	return headers.indexOf(th);
}

/** @param {HTMLTableElement} table */
function initializeWidths(table) {
	const headers = Array.from(table.tHead?.rows?.[0]?.cells ?? []);
	const cols = ensureColgroup(table);
	if (!cols || headers.length !== cols.length) return;

	headers.forEach((th, i) => {
		const min = Number.parseFloat(th.dataset.colMinWidth ?? "") || DEFAULT_MIN_COL_WIDTH;
		const width = Math.max(min, Math.ceil(th.getBoundingClientRect().width));
		cols[i].style.width = `${width}px`;
		cols[i].style.minWidth = `${min}px`;
	});
}

/**
 * @param {HTMLTableElement} table
 * @param {HTMLTableCellElement} th
 */
function attachResizeHandle(table, th) {
	const handle = document.createElement("span");
	handle.className = "col-resize-handle";
	handle.setAttribute("aria-hidden", "true");
	th.appendChild(handle);

	handle.addEventListener("pointerdown", (event) => {
		event.preventDefault();
		event.stopPropagation();

		const colIndex = getColumnIndex(table, th);
		if (colIndex < 0) return;

		const cols = ensureColgroup(table);
		if (!cols) return;

		const col = cols[colIndex];
		const min = Number.parseFloat(th.dataset.colMinWidth ?? "") || DEFAULT_MIN_COL_WIDTH;
		const startX = event.clientX;
		const startWidth = col.getBoundingClientRect().width;

		document.body.classList.add("is-col-resizing");
		th.classList.add("is-resizing");

		/** @param {PointerEvent} moveEvent */
		const onMove = (moveEvent) => {
			const nextWidth = Math.max(min, startWidth + (moveEvent.clientX - startX));
			col.style.width = `${Math.round(nextWidth)}px`;
			col.style.minWidth = `${min}px`;
			syncBrowserChipLayouts(table);
		};

		const onUp = () => {
			document.body.classList.remove("is-col-resizing");
			th.classList.remove("is-resizing");
			window.removeEventListener("pointermove", onMove);
			window.removeEventListener("pointerup", onUp);
		};

		window.addEventListener("pointermove", onMove);
		window.addEventListener("pointerup", onUp);
	});
}

/** @param {HTMLTableElement} table */
function enhanceTable(table) {
	initializeWidths(table);
	syncBrowserChipLayouts(table);
	const headers = Array.from(table.tHead?.rows?.[0]?.cells ?? []);
	if (headers.length === 0) return;

	let dragIndex = -1;

	for (const headerCell of headers) {
		const th = /** @type {HTMLTableCellElement} */ (headerCell);
		th.classList.add("table-col-header");
		th.draggable = true;

		attachResizeHandle(table, th);

		th.addEventListener("dragstart", (event) => {
			if (event.target instanceof Element && event.target.classList.contains("col-resize-handle")) {
				event.preventDefault();
				return;
			}
			dragIndex = getColumnIndex(table, th);
			if (dragIndex < 0) return;
			th.classList.add("is-dragging");
			if (event.dataTransfer) {
				event.dataTransfer.effectAllowed = "move";
				event.dataTransfer.setData("text/plain", String(dragIndex));
			}
		});

		th.addEventListener("dragend", () => {
			dragIndex = -1;
			th.classList.remove("is-dragging", "is-drop-target");
			for (const other of headers) {
				other.classList.remove("is-drop-target");
			}
		});

		th.addEventListener("dragover", (event) => {
			if (dragIndex < 0) return;
			event.preventDefault();
			th.classList.add("is-drop-target");
		});

		th.addEventListener("dragleave", () => {
			th.classList.remove("is-drop-target");
		});

		th.addEventListener("drop", (event) => {
			if (dragIndex < 0) return;
			event.preventDefault();
			const dropIndex = getColumnIndex(table, th);
			th.classList.remove("is-drop-target");
			if (dropIndex < 0 || dropIndex === dragIndex) return;
			moveColumn(table, dragIndex, dropIndex);
			syncBrowserChipLayouts(table);
			dragIndex = -1;
		});
	}
}

const resizeObserver = typeof ResizeObserver === "function"
	? new ResizeObserver((entries) => {
		for (const entry of entries) {
			if (entry.target instanceof Element) {
				syncBrowserChipLayouts(entry.target);
			}
		}
	})
	: null;

for (const table of document.querySelectorAll("section[data-searchable] table")) {
	if (!(table instanceof HTMLTableElement)) continue;
	enhanceTable(table);
	const resizeTarget = table.closest(".table-scroll") ?? table;
	resizeObserver?.observe(resizeTarget);
}

window.addEventListener("resize", () => syncBrowserChipLayouts(document));
