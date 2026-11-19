<script lang="ts">
	// Log in with a TrueNAS account: user name + password, then a one-time code if the account
	// asks for one. Outside the (app) group: no navigation here.
	import { onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '#lib/components/Icon.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import RestartScreen from '#lib/components/RestartScreen.svelte';
	import ThemeToggle from '#lib/components/ThemeToggle.svelte';
	import { loginWithCode, loginWithPassword, safeNext, type LoginResult } from '#lib/login.js';
	import { pageVisible } from '#lib/nas.svelte.js';
	import {
		backText,
		CERT_CHANGED,
		pollDelay,
		probeNas,
		reachSignal,
		type NasReach
	} from '#lib/restart.js';
	import { endRestartWait, markWaitDown, restart, useRestartWatch } from '#lib/restart.svelte.js';
	import { setSession } from '#lib/session.svelte.js';

	let step = $state<'password' | 'code'>('password');
	let username = $state('');
	let password = $state('');
	let code = $state('');
	let error = $state<string | null>(null);
	let busy = $state(false);

	let usernameInput = $state<HTMLInputElement>();
	let passwordInput = $state<HTMLInputElement>();
	let codeInput = $state<HTMLInputElement>();

	const ended = $derived(page.url.searchParams.get('ended') === '1');

	/**
	 * Waiting for a restarting NAS: the form never shows under a live wait (one left over from
	 * the app, or from before a reload).
	 */
	const waiting = $derived(!!restart.wait);
	/** The NAS answered again after a restart: say so above the form. */
	let back = $state<{ host: string | null } | null>(null);
	/** The last answer of `GET /api/nas`, `null` before the first. */
	let reach = $state<NasReach | null>(null);

	// No session: a restart always ends it (the NAS forgets truelite's token), so the NAS was
	// seen down. Also covers a reload during the wait, sent here by the (app) load.
	markWaitDown();

	// Ask whether the NAS is back, paced by restart.ts; paused while the page is hidden.
	$effect(() => {
		const wait = restart.wait;
		if (!wait || !pageVisible()) return;
		let stopped = false;
		let timer: ReturnType<typeof setTimeout> | undefined;
		const check = async () => {
			const answer = await probeNas();
			if (stopped) return;
			reach = answer;
			timer = setTimeout(check, pollDelay(wait.kind, Date.now() - wait.since));
		};
		check();
		return () => {
			stopped = true;
			clearTimeout(timer);
		};
	});

	useRestartWatch(
		() => reachSignal(reach),
		(reason, wait) => {
			if (reason === 'back') {
				back = { host: wait.host };
				if (reach === 'cert_mismatch') error = CERT_CHANGED;
			}
			focusStep();
		}
	);

	function stopWaiting() {
		endRestartWait();
		focusStep();
	}

	/** Focuses the first field a step needs once it is on screen. */
	async function focusStep() {
		await tick();
		if (step === 'code') codeInput?.focus();
		else if (username) passwordInput?.focus();
		else usernameInput?.focus();
	}

	onMount(focusStep);

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (busy) return;
		busy = true;
		error = null;
		try {
			const result =
				step === 'password'
					? await loginWithPassword(username.trim(), password)
					: await loginWithCode(code);
			await handle(result);
		} finally {
			busy = false;
		}
	}

	async function handle(result: LoginResult) {
		switch (result.kind) {
			case 'done':
				// Logged in: the NAS answers, so any wait is over (none outlives a login).
				endRestartWait('back');
				setSession(result.user);
				await goto(safeNext(page.url.searchParams.get('next')), { replaceState: true });
				return;
			case 'otp':
				// The password is no longer needed: the server keeps the half-done login.
				password = '';
				code = '';
				error = result.error ?? null;
				step = 'code';
				break;
			case 'restart':
				code = '';
				error = result.error;
				step = 'password';
				break;
			case 'error':
				error = result.error;
				if (step === 'password') password = '';
				else code = '';
				break;
		}
		await focusStep();
	}

	function useAnotherAccount() {
		step = 'password';
		code = '';
		error = null;
		focusStep();
	}
</script>

<svelte:head>
	<!-- The restart screen names the page itself. -->
	{#if !waiting}
		<title>Log in · truelite</title>
	{/if}
</svelte:head>

{#if restart.wait}
	<RestartScreen wait={restart.wait} onstop={stopWaiting} />
{:else}
	<main class="page">
		<ThemeToggle />
		<div class="card">
			<header>
				<Logo size={40} />
				<h1>Log in</h1>
				<p class="lead">
					{step === 'password'
						? 'Use your TrueNAS account.'
						: 'Enter the code from your authenticator app.'}
				</p>
			</header>

			{#if back && !error}
				<p class="notice" role="status">
					<Icon name="info" size={16} />
					<span>{backText(back.host)}</span>
				</p>
			{:else if ended && !error}
				<p class="notice" role="status">
					<Icon name="info" size={16} />
					<span>Your session ended. Log in again.</span>
				</p>
			{/if}

			<form method="post" onsubmit={submit}>
				{#if step === 'password'}
					<label class="field">
						<span>User name</span>
						<input
							bind:this={usernameInput}
							bind:value={username}
							name="username"
							autocomplete="username"
							autocapitalize="none"
							spellcheck="false"
							required
							aria-invalid={error ? 'true' : undefined}
							aria-describedby={error ? 'login-error' : undefined}
						/>
					</label>
					<label class="field">
						<span>Password</span>
						<input
							bind:this={passwordInput}
							bind:value={password}
							type="password"
							name="password"
							autocomplete="current-password"
							required
							aria-invalid={error ? 'true' : undefined}
							aria-describedby={error ? 'login-error' : undefined}
						/>
					</label>
				{:else}
					<!-- Hidden user name: password managers file the code under the right account. -->
					<input type="hidden" name="username" value={username} autocomplete="username" />
					<label class="field">
						<span>Code</span>
						<input
							bind:this={codeInput}
							bind:value={() => code, (v) => (code = v.replace(/\s+/g, ''))}
							name="code"
							inputmode="numeric"
							autocomplete="one-time-code"
							pattern={'[0-9]{6,8}'}
							required
							aria-invalid={error ? 'true' : undefined}
							aria-describedby={error ? 'login-error' : undefined}
						/>
					</label>
				{/if}

				{#if error}
					<p class="alert" id="login-error" role="alert">
						<Icon name="circle-alert" size={16} />
						<span>{error}</span>
					</p>
				{/if}

				<button class="primary" type="submit" disabled={busy} aria-busy={busy}>
					{#if step === 'password'}
						{busy ? 'Logging in…' : 'Log in'}
					{:else}
						{busy ? 'Checking…' : 'Verify'}
					{/if}
				</button>

				{#if step === 'code'}
					<button class="text" type="button" onclick={useAnotherAccount} disabled={busy}>
						Use another account
					</button>
				{/if}
			</form>
		</div>
	</main>
{/if}

<style>
	.page {
		display: grid;
		place-items: center;
		min-height: 100dvh;
		padding: max(var(--space-6), env(safe-area-inset-top))
			max(var(--space-4), env(safe-area-inset-right))
			max(var(--space-6), env(safe-area-inset-bottom))
			max(var(--space-4), env(safe-area-inset-left));
		background: var(--surface);
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-5);
		width: 100%;
		max-width: 24rem;
		padding: var(--space-6) var(--space-5);
		border-radius: var(--radius-xl);
		background: var(--container);
		box-shadow: var(--shadow-border-xs);
	}
	header {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		text-align: center;
	}
	h1 {
		margin: var(--space-2) 0 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-medium);
	}
	.lead {
		margin: 0;
		color: var(--text-secondary);
	}
	form {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	/* Form field: a bordered box with a small label above a borderless input. */
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: var(--container);
		box-shadow: var(--shadow-xs);
		cursor: text;
		/* The focus recipe of base.css, on the box instead of the borderless input. */
		outline: 2px solid transparent;
		outline-offset: 0;
	}
	.field:has(input:focus-visible) {
		outline-color: var(--focus-ring);
	}
	.field span {
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.field input {
		width: 100%;
		min-height: 1.5rem;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--text-primary);
		font: inherit;
		/* 16 px: iOS zooms into smaller fields. */
		font-size: var(--text-base);
	}
	.field input:focus-visible {
		/* The box's halo shows focus. */
		outline: none;
	}
	.field input[name='code'] {
		font-family: var(--font-mono);
		letter-spacing: 0.2em;
	}

	.alert,
	.notice {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-3);
		border-radius: var(--radius-lg);
		color: var(--text-primary);
	}
	.alert {
		border: 1px solid color-mix(in srgb, var(--destructive) 40%, transparent);
		background: var(--destructive-tint);
	}
	.alert :global(svg),
	.notice :global(svg) {
		margin-top: 0.125rem;
	}
	.alert :global(svg) {
		color: var(--destructive);
	}
	.notice {
		background: var(--container-inset);
		color: var(--text-secondary);
	}

	.primary {
		width: 100%;
		min-height: 3rem;
		margin-top: var(--space-1);
		border: 0;
		border-radius: var(--radius-lg);
		background: var(--button-primary-bg);
		color: var(--button-primary-text);
		font-weight: var(--weight-medium);
		cursor: pointer;
		transition: opacity var(--duration-fast);
	}
	.primary:disabled {
		opacity: 0.7;
		cursor: progress;
	}
	.text {
		align-self: center;
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-secondary);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	.text:hover:not(:disabled) {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.text {
			min-height: var(--touch);
		}
	}
</style>
