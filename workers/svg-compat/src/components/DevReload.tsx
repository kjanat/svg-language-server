// @ts-nocheck Deno
interface Props {
	dev: boolean;
	boot: number;
}

/**
 * DEV-only reload poller. Fetches /__reload every 500ms and reloads the
 * page when the BOOT timestamp changes. Only emitted when `dev` is true,
 * so production pages never ship this script.
 *
 * CSP note: this loads a static module and passes BOOT via a data attribute,
 * avoiding inline script construction in rendered HTML.
 */
export function DevReload({ dev, boot }: Props) {
	if (!dev) return null;
	return <script type='module' src='/dev-reload.mjs' data-boot={String(boot)}></script>;
}
