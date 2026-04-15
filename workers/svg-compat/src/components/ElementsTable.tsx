import { elementSearchTokens, type NamedCompatEntry } from "../view.ts";
import { BaselineBadge } from "./BaselineBadge.tsx";
import { BrowserSupport } from "./BrowserSupport.tsx";
import { DocsLinks } from "./DocsLinks.tsx";

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
								<DocsLinks
									mdnUrl={entry.mdn_url}
									specUrls={entry.spec_url}
									deprecated={entry.deprecated}
								/>
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
