<script lang="ts">
	import type { Snippet } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { NAV_ITEMS, isActive } from '#lib/nav.js';
	import { nas } from '#lib/nas.svelte.js';
	import { logout, session } from '#lib/session.svelte.js';
	import type { Theme } from '#lib/theme.js';
	import { setTheme, theme } from '#lib/theme.svelte.js';
	import ConnectionBanner from './ConnectionBanner.svelte';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	let { children }: { children: Snippet } = $props();

	const THEME_OPTIONS: { value: Theme; label: string; icon: 'monitor' | 'sun' | 'moon' }[] = [
		{ value: 'system', label: 'System', icon: 'monitor' },
		{ value: 'light', label: 'Light', icon: 'sun' },
		{ value: 'dark', label: 'Dark', icon: 'moon' }
	];

	let accountOpen = $state(false);
	let account = $state<HTMLDetailsElement>();
	let loggingOut = $state(false);

	// Open the connection to truelite; the layout's load already learnt who is logged in.
	$effect(() => {
		nas();
	});

	// A navigation closes the account menu.
	$effect(() => {
		void page.url.pathname;
		accountOpen = false;
	});

	// While open, a click elsewhere or Escape closes the account menu.
	$effect(() => {
		if (!accountOpen) return;
		const onPointer = (e: PointerEvent) => {
			if (account && !account.contains(e.target as Node)) accountOpen = false;
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key !== 'Escape') return;
			accountOpen = false;
			account?.querySelector('summary')?.focus();
		};
		document.addEventListener('pointerdown', onPointer);
		document.addEventListener('keydown', onKey);
		return () => {
			document.removeEventListener('pointerdown', onPointer);
			document.removeEventListener('keydown', onKey);
		};
	});

	async function onLogout() {
		loggingOut = true;
		try {
			await logout();
			await goto('/login');
		} finally {
			loggingOut = false;
		}
	}
</script>

<a class="skip" href="#main">Skip to content</a>

<nav class="nav" aria-label="Main">
	<a class="brand" href="/" aria-label="truelite, dashboard"><Logo /></a>
	<ul class="items">
		{#each NAV_ITEMS as item (item.href)}
			<li>
				<a
					class="item"
					href={item.href}
					aria-current={isActive(page.url.pathname, item.href) ? 'page' : undefined}
				>
					<span class="icon"><Icon name={item.icon} /></span>
					<span class="label">{item.label}</span>
				</a>
			</li>
		{/each}
		<li class="account">
			<details bind:this={account} bind:open={accountOpen}>
				<summary class="item">
					<span class="icon"><Icon name="circle-user" /></span>
					<span class="label">Account</span>
				</summary>
				<div class="menu">
					<p class="who">
						{#if session.user}
							Logged in as <strong>{session.user.username}</strong>
						{:else}
							Not logged in
						{/if}
					</p>
					<fieldset class="theme">
						<legend>Theme</legend>
						<div class="segments menu-segments">
							{#each THEME_OPTIONS as option (option.value)}
								<label class="segment">
									<input
										class="visually-hidden"
										type="radio"
										name="theme"
										value={option.value}
										checked={theme.value === option.value}
										onchange={() => setTheme(option.value)}
									/>
									<Icon name={option.icon} size={16} />
									{option.label}
								</label>
							{/each}
						</div>
					</fieldset>
					<button class="menu-item" type="button" onclick={onLogout} disabled={loggingOut}>
						<Icon name="log-out" size={16} />
						{loggingOut ? 'Logging out…' : 'Log out'}
					</button>
				</div>
			</details>
		</li>
	</ul>
</nav>

<ThemeToggle />

<div class="content">
	<ConnectionBanner />
	<main id="main" tabindex="-1">
		{@render children()}
	</main>
</div>

<style>
	/* Skip link: off screen until focused. */
	.skip {
		position: fixed;
		top: max(var(--space-2), env(safe-area-inset-top));
		left: max(var(--space-2), env(safe-area-inset-left));
		z-index: 40;
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-lg);
		background: var(--button-primary-bg);
		color: var(--button-primary-text);
		font-weight: var(--weight-medium);
		text-decoration: none;
		transform: translateY(-200%);
	}
	.skip:focus {
		transform: none;
	}

	/* Mobile first: a bottom tab bar. */
	.nav {
		position: fixed;
		inset: auto 0 0 0;
		z-index: 20;
		padding: 0 env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left);
		background: var(--container);
		box-shadow: 0 -1px 0 var(--border-subdued);
	}
	.brand {
		display: none;
	}
	.items {
		display: flex;
		height: var(--tabbar-height);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.items > li {
		position: relative;
		display: flex;
		flex: 1;
		min-width: 0;
	}
	.item {
		position: relative;
		display: flex;
		flex: 1;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.125rem;
		min-height: var(--touch);
		color: var(--text-secondary);
		font-size: var(--text-2xs);
		font-weight: var(--weight-medium);
		line-height: 1.2;
		text-decoration: none;
		list-style: none;
		cursor: pointer;
		user-select: none;
		transition: color var(--duration-fast);
	}
	summary.item::-webkit-details-marker {
		display: none;
	}
	.item:hover,
	.item[aria-current='page'],
	details[open] > .item {
		color: var(--text-primary);
	}
	.icon {
		display: grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border-radius: var(--radius-lg);
		transition:
			background-color var(--duration-fast),
			box-shadow var(--duration-fast);
	}
	.item[aria-current='page'] .icon,
	details[open] > .item .icon {
		background: var(--container-inset);
	}
	.label {
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.account details {
		display: flex;
		flex: 1;
	}

	/* Account menu: opens above the tab bar, aligned to the right edge. */
	.menu {
		position: absolute;
		right: var(--space-2);
		bottom: calc(100% + var(--space-2));
		z-index: 30;
		min-width: 14rem;
		max-width: calc(100vw - 2 * var(--space-2));
		padding: var(--space-2);
		border-radius: var(--radius-lg);
		background: var(--container);
		box-shadow: var(--shadow-lg);
	}
	.who {
		margin: 0;
		padding: var(--space-2) var(--space-3);
		color: var(--text-secondary);
		font-size: var(--text-xs);
		overflow-wrap: anywhere;
	}
	.who strong {
		display: block;
		color: var(--text-primary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
	}
	/* Theme: a segmented control of native radios (arrow keys, screen readers). */
	.theme {
		margin: 0 0 var(--space-2);
		padding: 0 0 var(--space-2);
		border: 0;
		border-bottom: 1px solid var(--border-subdued);
	}
	.menu-segments {
		margin: 0 var(--space-1);
	}
	.theme legend {
		padding: var(--space-1) var(--space-3);
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.menu-item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-primary);
		font-weight: var(--weight-medium);
		text-align: left;
		cursor: pointer;
		transition: background-color var(--duration-fast);
	}
	.menu-item:hover:not(:disabled) {
		background: var(--container-inset-hover);
	}
	.menu-item:disabled {
		color: var(--text-secondary);
		cursor: progress;
	}
	@media (pointer: coarse) {
		.menu-item {
			min-height: var(--touch);
		}
	}

	/* Content clears the tab bar and the notch. */
	.content {
		min-height: 100dvh;
		padding-bottom: calc(var(--tabbar-height) + env(safe-area-inset-bottom));
	}
	main {
		max-width: var(--content-max);
		margin: 0 auto;
		padding: max(var(--gutter), env(safe-area-inset-top))
			max(var(--gutter), env(safe-area-inset-right)) var(--space-8)
			max(var(--gutter), env(safe-area-inset-left));
	}
	main:focus {
		outline: none;
	}

	/* Desktop: an icon rail on the left. */
	@media (min-width: 1024px) {
		.nav {
			inset: 0 auto 0 0;
			display: flex;
			flex-direction: column;
			align-items: stretch;
			width: calc(var(--rail-width) + env(safe-area-inset-left));
			padding: max(var(--space-4), env(safe-area-inset-top)) 0
				max(var(--space-4), env(safe-area-inset-bottom)) env(safe-area-inset-left);
			background: var(--surface);
			box-shadow: 1px 0 0 var(--border-subdued);
		}
		.brand {
			display: grid;
			place-items: center;
			align-self: center;
			margin-bottom: var(--space-6);
			border-radius: var(--radius-lg);
		}
		.items {
			flex: 1;
			flex-direction: column;
			gap: var(--space-2);
			height: auto;
		}
		.items > li {
			flex: none;
		}
		.account {
			margin-top: auto;
		}
		.item {
			justify-content: flex-start;
			padding: var(--space-1) var(--space-1) var(--space-1);
		}
		.item:hover .icon {
			background: var(--surface-hover);
		}
		.item[aria-current='page'] .icon,
		details[open] > .item .icon {
			background: var(--container);
			box-shadow: var(--shadow-border-xs);
		}
		/* Active indicator: a small bar on the rail's left edge. */
		.item[aria-current='page']::before {
			content: '';
			position: absolute;
			top: calc(var(--space-1) + 0.5rem);
			left: 0;
			width: 4px;
			height: 1rem;
			border-radius: 0 2px 2px 0;
			background: var(--nav-indicator);
		}
		.menu {
			right: auto;
			bottom: 0;
			left: calc(100% + var(--space-2));
		}
		.content {
			margin-left: calc(var(--rail-width) + env(safe-area-inset-left));
			padding-bottom: 0;
		}
		main {
			padding-top: max(var(--space-8), env(safe-area-inset-top));
		}
	}
</style>
