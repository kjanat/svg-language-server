import type { Baseline } from "../main.ts";

interface Props {
	baseline: Baseline | undefined;
}

const BADGE_SRC = {
	widely: "/badges/baseline-widely.svg",
	newly: "/badges/baseline-newly.svg",
	limited: "/badges/baseline-limited.svg",
} as const;

export function BaselineBadge({ baseline }: Props) {
	if (!baseline) return <span class="muted">-</span>;
	const variant = baseline.status;
	const label = variant === "limited"
		? "limited"
		: `${variant} ${baseline.since ?? ""}`.trim();
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
