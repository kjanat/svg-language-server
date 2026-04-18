interface Props {
	dev: boolean;
	boot: number;
}

/**
 * DEV-only reload poller. Fetches /__reload every 500ms and reloads the
 * page when the BOOT timestamp changes. Only emitted when `dev` is true,
 * so production pages never ship this script.
 *
 * CSP note: this is an inline script. http.ts adds `'unsafe-inline'` to
 * script-src only when DEV is true, so this component only renders under
 * the same condition.
 */
export function DevReload({ dev, boot }: Props) {
	if (!dev) return null;
	const body =
		`((b)=>{setInterval(()=>fetch("/__reload").then(r=>r.text()).then(t=>{if(t!==b){b=t;location.reload()}}),500)})(${
			JSON.stringify(String(boot))
		})`;
	return <script dangerouslySetInnerHTML={{ __html: body }} />;
}
