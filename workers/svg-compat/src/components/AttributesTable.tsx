import { attributeSearchTokens, type NamedAttributeEntry } from "../view.ts";
import { BaselineBadge } from "./BaselineBadge.tsx";

interface Props {
	rows: NamedAttributeEntry[];
}

function formatScope(elements: string[]): string {
	if (elements.length === 1 && elements[0] === "*") return "global";
	return elements.join(", ");
}

export function AttributesTable({ rows }: Props) {
	return (
		<div class="table-scroll">
			<table>
				<thead>
					<tr>
						<th scope="col">Name</th>
						<th scope="col">Elements</th>
						<th scope="col">Baseline</th>
						<th scope="col">Docs</th>
					</tr>
				</thead>
				<tbody>
					{rows.map((entry) => (
						<tr data-search={attributeSearchTokens(entry)}>
							<th scope="row">
								<code>{entry.name}</code>
							</th>
							<td class="scope-cell">{formatScope(entry.elements)}</td>
							<td>
								<BaselineBadge baseline={entry.baseline} />
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
