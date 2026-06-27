// @ts-nocheck Deno
import { BaselineBadge } from '#component/BaselineBadge.tsx';
import { BrowserSupport } from '#component/BrowserSupport.tsx';
import { DocsLinks } from '#component/DocsLinks.tsx';
import { elementSearchTokens, type NamedCompatEntry } from '#src/view.ts';

interface Props {
	rows: NamedCompatEntry[];
}

export function ElementsTable({ rows }: Props) {
	return (
		<div class='table-scroll'>
			<table>
				<thead>
					<tr>
						<th scope='col'>Name</th>
						<th scope='col'>Baseline</th>
						<th scope='col'>Support</th>
						<th scope='col'>Docs</th>
					</tr>
				</thead>
				<tbody>
					{rows.map((entry) => (
						<tr data-search={elementSearchTokens(entry)}>
							<th scope='row'>
								<code>{entry.name}</code>
							</th>
							<td>
								<BaselineBadge baseline={entry.baseline} />
							</td>
							<td>
								<BrowserSupport support={entry.browser_support} baselineStatus={entry.baseline?.status} />
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
					<tr class='table-empty' hidden>
						<td colspan={4}>No matches.</td>
					</tr>
				</tbody>
			</table>
		</div>
	);
}
