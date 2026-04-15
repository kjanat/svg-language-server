import type { ComponentChildren } from "preact";

interface Props {
	id: string;
	title: string;
	description: string;
	total: number;
	placeholder: string;
	children: ComponentChildren;
}

/**
 * Section shell with header, live-filter input, counter, and a child
 * table. Emits its own `<script type="module" src="/table-filter.mjs">`
 * so the section is self-contained — no Layout coordination needed.
 * Multiple instances are free: module scripts dedupe by URL, so the
 * filter module evaluates exactly once per page.
 *
 * The `id` must match the `data-filter-target` attribute on the search
 * input so the filter script can find the table rows to toggle.
 */
export function TableSection(
	{ id, title, description, total, placeholder, children }: Props,
) {
	return (
		<section id={id} data-searchable>
			<header class="section-head">
				<div>
					<h2>{title}</h2>
					<p class="muted">{description}</p>
				</div>
				<label class="table-search">
					<input
						type="search"
						placeholder={placeholder}
						data-filter-target={id}
						aria-label={placeholder}
					/>
					<span class="table-search-count" data-filter-count={id}>
						{total} / {total}
					</span>
				</label>
			</header>
			{children}
			<script type="module" src="/table-filter.mjs"></script>
			<script type="module" src="/table-columns.mjs"></script>
		</section>
	);
}
