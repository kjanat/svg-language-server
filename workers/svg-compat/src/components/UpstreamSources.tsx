import type { SourceInfo } from "../sources.ts";
import { sourceSearchTokens } from "../view.ts";
import { TableSection } from "./TableSection.tsx";

interface Props {
	sources: SourceInfo[];
}

export function UpstreamSources({ sources }: Props) {
	return (
		<TableSection
			id="sources"
			title="Upstream sources"
			description="Effective package versions for this exact response."
			total={sources.length}
			placeholder="Filter sources…"
		>
			<div class="table-scroll">
				<table>
					<thead>
						<tr>
							<th scope="col">Package</th>
							<th scope="col">Requested</th>
							<th scope="col">Resolved</th>
							<th scope="col">Mode</th>
							<th scope="col">Link</th>
						</tr>
					</thead>
					<tbody>
						{sources.map((source) => (
							<tr data-search={sourceSearchTokens(source)}>
								<th scope="row">
									<code>{source.package}</code>
								</th>
								<td>{source.requested}</td>
								<td>{source.resolved}</td>
								<td>{source.mode}</td>
								<td>
									<a href={source.source_url}>source</a>
								</td>
							</tr>
						))}
						<tr class="table-empty" hidden>
							<td colspan={5}>No matches.</td>
						</tr>
					</tbody>
				</table>
			</div>
		</TableSection>
	);
}
