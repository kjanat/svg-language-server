interface Props {
	status: number;
	message: string;
}

export function ErrorPage({ status, message }: Props) {
	return (
		<main class="error-page">
			<article>
				<p class="eyebrow">Error {status}</p>
				<h1>Request failed</h1>
				<p>{message}</p>
				<p class="muted">
					Ask for <code>application/json</code>, <code>/data.json</code>, or <code>text/html</code>.
				</p>
			</article>
		</main>
	);
}
