import type { PageStats } from "../view.ts";

interface Props {
	stats: PageStats;
}

interface StatCell {
	value: number;
	label: string;
	/**
	 * Secondary tiles (new signal counts) are rendered with a muted
	 * accent so the structural "elements / attributes" counts stay the
	 * primary eye-magnet of the hero grid.
	 */
	secondary?: boolean;
}

export function StatsGrid({ stats }: Props) {
	// Primary row: structural counts + the two classic quality buckets.
	const primary: StatCell[] = [
		{ value: stats.elements, label: "elements" },
		{ value: stats.attributes, label: "attributes" },
		{ value: stats.deprecated, label: "deprecated" },
		{ value: stats.limited, label: "limited" },
	];
	// Secondary row: new signals preserved end-to-end from BCD. Rendered
	// in a lower-contrast style to give sighted users a quick inventory
	// of the signal richness the data carries.
	const secondary: StatCell[] = [
		{ value: stats.partial, label: "partial", secondary: true },
		{ value: stats.removed, label: "removed", secondary: true },
		{ value: stats.flagged, label: "flagged", secondary: true },
		{ value: stats.unsupportedSomewhere, label: "unsupported", secondary: true },
	];
	return (
		<div class="stats">
			{primary.map((cell) => (
				<div class="stat">
					<strong>{cell.value}</strong>
					<span class="stat-label">{cell.label}</span>
				</div>
			))}
			{secondary.map((cell) => (
				<div class="stat stat-secondary">
					<strong>{cell.value}</strong>
					<span class="stat-label">{cell.label}</span>
				</div>
			))}
		</div>
	);
}
