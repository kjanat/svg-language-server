import type { Baseline } from "../main.ts";

interface Props {
	baseline: Baseline | undefined;
}

const BADGE_SRC = {
	widely: "/badges/baseline-widely.svg",
	newly: "/badges/baseline-newly.svg",
	limited: "/badges/baseline-limited.svg",
} as const;

/**
 * Glyph rendered before the year when `since_qualifier` is set —
 * mirrors the upstream web-features prefix so `≤2021-04-02`
 * surfaces in the badge as `≤2021` rather than silently
 * displaying as `2021`. Unknown qualifiers fall through to `~`
 * (the parser already mapped them to "approximately").
 */
const QUALIFIER_GLYPH: Record<NonNullable<Baseline["since_qualifier"]>, string> = {
	before: "≤",
	after: "≥",
	approximately: "~",
};

export function BaselineBadge({ baseline }: Props) {
	if (!baseline) return <span class="muted">-</span>;
	const variant = baseline.status;
	const glyph = baseline.since_qualifier ? QUALIFIER_GLYPH[baseline.since_qualifier] : "";
	const label = variant === "limited"
		? "limited"
		: `${variant} ${glyph}${baseline.since ?? ""}`.trim();
	return (
		<span class={`badge badge-${variant}`}>
			<img
				class="badge-icon"
				src={BADGE_SRC[variant]}
				alt=""
				width="18"
				height="10"
			/>
			{label}
		</span>
	);
}
