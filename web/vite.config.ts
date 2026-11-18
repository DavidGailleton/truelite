/// <reference types="vitest/config" />
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Where the Rust server runs in development (compose.dev.yaml sets it to the server container).
const backend = process.env.TRUELITE_BACKEND ?? 'http://127.0.0.1:8080';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Single-page app: the Rust server serves index.html for every unknown path.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	},
	server: {
		proxy: {
			'/api': backend,
			'/ws': { target: backend.replace(/^http/, 'ws'), ws: true }
		}
	}
});
