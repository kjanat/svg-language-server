/** @type {NodeListOf<HTMLInputElement>} */
const inputs = document.querySelectorAll("input.ver[data-pkg]");
const searchParams = new URLSearchParams(location.search);
for (const input of inputs) {
	const v = searchParams.get(input.dataset.param);
	if (v) input.value = v;
}

function navigate() {
	const params = new URLSearchParams(location.search);
	let any = false;
	for (const input of inputs) {
		const v = input.value.trim();
		if (v) {
			params.set(input.dataset.param, v);
			any = true;
		}
	}
	if (any) location.search = params.toString();
}

for (const input of inputs) {
	const list = input.nextElementSibling;
	let allVersions = [];
	let active = -1;

	input.setAttribute("role", "combobox");
	input.setAttribute("aria-autocomplete", "list");
	input.setAttribute("aria-expanded", "false");
	list.setAttribute("role", "listbox");

	const CACHE_TTL = 30 * 60 * 1000;
	const cacheKey = `ver:${input.dataset.pkg}`;

	try {
		const cached = JSON.parse(localStorage.getItem(cacheKey));
		if (cached && Date.now() < cached.exp) allVersions = cached.v;
	} catch {}

	if (!allVersions.length) {
		fetch("https://data.jsdelivr.com/v1/package/npm/" + input.dataset.pkg)
			.then((r) => r.json())
			.then((data) => {
				allVersions = data.versions;
				try {
					localStorage.setItem(cacheKey, JSON.stringify({ v: allVersions, exp: Date.now() + CACHE_TTL }));
				} catch {}
			})
			.catch(() => {});
	}

	function show(filter) {
		list.innerHTML = "";
		active = -1;
		const q = filter.toLowerCase();
		const matches = q
			? allVersions
				.filter((v) => v.includes(q))
				.sort((a, b) => {
					const as = a.startsWith(q),
						bs = b.startsWith(q);
					if (as !== bs) return as ? -1 : 1;
					const ap = a.includes("-"),
						bp = b.includes("-");
					if (ap !== bp) return ap ? 1 : -1;
					return 0;
				})
			: allVersions;
		for (const v of matches.slice(0, 100)) {
			const li = document.createElement("li");
			li.textContent = v;
			li.setAttribute("role", "option");
			li.onmousedown = (e) => {
				e.preventDefault();
				pick(v);
			};
			list.appendChild(li);
		}
		const open = matches.length > 0;
		list.hidden = !open;
		input.setAttribute("aria-expanded", String(open));
	}

	function pick(v) {
		input.value = v;
		list.hidden = true;
		input.setAttribute("aria-expanded", "false");
		active = -1;
		navigate();
	}

	function highlight(idx) {
		const items = list.children;
		if (!items.length) return;
		if (active >= 0 && items[active]) items[active].classList.remove("active");
		active = ((idx % items.length) + items.length) % items.length;
		items[active].classList.add("active");
		items[active].scrollIntoView({ block: "nearest" });
	}

	input.addEventListener("focus", () => show(input.value));
	input.addEventListener("input", () => show(input.value));
	input.addEventListener("blur", () => {
		list.hidden = true;
		input.setAttribute("aria-expanded", "false");
		active = -1;
	});
	input.addEventListener("keydown", (e) => {
		if (e.key === "Enter" && active < 0) {
			navigate();
			return;
		}
		if (list.hidden && e.key !== "ArrowDown" && e.key !== "Tab") return;
		if (
			e.key === "ArrowDown"
			|| (e.key === "Tab" && !e.shiftKey && !list.hidden)
		) {
			e.preventDefault();
			if (list.hidden) show(input.value);
			else highlight(active + 1);
		} else if (
			e.key === "ArrowUp"
			|| (e.key === "Tab" && e.shiftKey && !list.hidden)
		) {
			e.preventDefault();
			highlight(active - 1);
		} else if (e.key === "Enter" && active >= 0) {
			e.preventDefault();
			pick(list.children[active].textContent);
		} else if (e.key === "Escape") {
			list.hidden = true;
			input.setAttribute("aria-expanded", "false");
			active = -1;
		}
	});
}
