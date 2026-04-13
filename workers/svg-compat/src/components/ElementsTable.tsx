import { elementSearchTokens, type NamedCompatEntry } from "../view.ts";
import { BaselineBadge } from "./BaselineBadge.tsx";
import { BrowserSupport } from "./BrowserSupport.tsx";

interface Props {
	rows: NamedCompatEntry[];
}

export function ElementsTable({ rows }: Props) {
	return (
		<div class="table-scroll">
			<table>
				<thead>
					<tr>
						<th scope="col">Name</th>
						<th scope="col">Baseline</th>
						<th scope="col">Support</th>
						<th scope="col">Docs</th>
					</tr>
				</thead>
				<tbody>
					{rows.map((entry) => (
						<tr data-search={elementSearchTokens(entry)}>
							<th scope="row">
								<code>{entry.name}</code>
							</th>
							<td>
								<BaselineBadge baseline={entry.baseline} />
							</td>
							<td>
								<BrowserSupport support={entry.browser_support} />
							</td>
							<td>
								{entry.mdn_url ? <a href={entry.mdn_url}>MDN</a> : <span class="muted">-</span>}
							</td>
						</tr>
					))}
					<tr class="table-empty" hidden>
						<td colspan={4}>No matches.</td>
					</tr>
				</tbody>
			</table>
		</div>
	);
}
