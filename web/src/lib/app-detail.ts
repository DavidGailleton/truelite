// One app in detail from `app.query` (api.truenas.com v25.10; role APPS_READ): its containers,
// published ports, mounted folders and web UI links. Framework-free so it can be unit-tested:
// the Svelte binding lives in app-detail.svelte.ts.
import { APP_SELECT, parseApp, type App, type Tone } from './apps';
import { isNum, isObj, isStr } from './system';

/**
 * Fields asked of `app.query` for one app. A dotted key selects one sub-key (TrueNAS query
 * options), so the catalog metadata and the workload summaries stay off the wire. Arrays can't be
 * narrowed: `container_details` comes whole, mounts included.
 */
export const APP_DETAIL_SELECT = [
	...APP_SELECT,
	'active_workloads.container_details',
	'active_workloads.used_ports',
	'portals',
	'metadata.description'
] as const;

/** `app.query` params for the app named `id` (a list of 0 or 1 entries). */
export function detailQuery(id: string): unknown[] {
	return [[['id', '=', id]], { select: [...APP_DETAIL_SELECT] }];
}

export type ContainerState = 'running' | 'starting' | 'created' | 'exited' | 'crashed' | 'unknown';

const CONTAINER_STATES: readonly ContainerState[] = [
	'running',
	'starting',
	'created',
	'exited',
	'crashed'
];

export interface Mount {
	/** Path on the NAS. */
	source: string;
	/** Path inside the container. */
	destination: string;
	readOnly: boolean;
}

export interface Container {
	/** Docker container id (the logs of #32 need it). */
	id: string;
	/** The compose service: `jellyfin`, `permissions`. */
	service: string;
	/** `jellyfin/jellyfin:10.11.2`; `null` if unknown. */
	image: string | null;
	state: ContainerState;
	mounts: Mount[];
}

export interface Port {
	/** Ports on the NAS, without the IPv4/IPv6 duplicates. */
	hostPorts: number[];
	containerPort: number;
	protocol: string;
}

export interface Portal {
	name: string;
	url: string;
}

export interface AppDetail {
	app: App;
	/** The catalog description; `null` for custom apps. */
	description: string | null;
	portals: Portal[];
	containers: Container[];
	ports: Port[];
}

function parseMount(raw: unknown): Mount | null {
	if (!isObj(raw) || !isStr(raw.source) || !isStr(raw.destination)) return null;
	if (raw.source === '' || raw.destination === '') return null;
	return { source: raw.source, destination: raw.destination, readOnly: raw.mode === 'ro' };
}

function parseContainer(raw: unknown): Container | null {
	if (!isObj(raw) || !isStr(raw.id) || raw.id === '') return null;
	const state = isStr(raw.state) ? raw.state.toLowerCase() : '';
	const mounts = Array.isArray(raw.volume_mounts) ? raw.volume_mounts : [];
	return {
		id: raw.id,
		service:
			isStr(raw.service_name) && raw.service_name !== '' ? raw.service_name : raw.id.slice(0, 12),
		image: isStr(raw.image) && raw.image !== '' ? raw.image : null,
		state: CONTAINER_STATES.find((s) => s === state) ?? 'unknown',
		mounts: mounts.map(parseMount).filter((m): m is Mount => m !== null)
	};
}

const isPort = (v: unknown): v is number => isNum(v) && Number.isInteger(v) && v > 0 && v < 65536;

/** `used_ports`: one row per container port and protocol, host ports listed once each. */
export function parsePorts(raw: unknown): Port[] {
	if (!Array.isArray(raw)) return [];
	const rows = new Map<string, Port>();
	for (const entry of raw) {
		if (!isObj(entry) || !isPort(entry.container_port)) continue;
		const protocol = isStr(entry.protocol) && entry.protocol !== '' ? entry.protocol : 'tcp';
		const key = `${entry.container_port}/${protocol}`;
		const row = rows.get(key) ?? { hostPorts: [], containerPort: entry.container_port, protocol };
		rows.set(key, row);
		const hosts = Array.isArray(entry.host_ports) ? entry.host_ports : [];
		for (const host of hosts) {
			if (isObj(host) && isPort(host.host_port) && !row.hostPorts.includes(host.host_port)) {
				row.hostPorts.push(host.host_port);
			}
		}
	}
	return [...rows.values()]
		.map((p) => ({ ...p, hostPorts: p.hostPorts.sort((a, b) => a - b) }))
		.sort((a, b) => a.containerPort - b.containerPort || a.protocol.localeCompare(b.protocol));
}

const ANY_HOST = new Set(['0.0.0.0', '[::]', '::']);

/**
 * A portal URL the browser can open: TrueNAS writes the bind address (`http://0.0.0.0:30013/`),
 * which becomes `hostname` (truelite runs on the NAS, so it's the name the user reached it by).
 * `null` for anything but http(s).
 */
export function portalUrl(raw: unknown, hostname: string): string | null {
	if (!isStr(raw)) return null;
	let url: URL;
	try {
		url = new URL(raw);
	} catch {
		return null;
	}
	if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
	if (ANY_HOST.has(url.hostname) && hostname !== '') url.hostname = hostname;
	return url.href;
}

function parsePortals(raw: unknown, hostname: string): Portal[] {
	if (!isObj(raw)) return [];
	const portals: Portal[] = [];
	for (const [name, value] of Object.entries(raw)) {
		const url = portalUrl(value, hostname);
		if (url && name.trim() !== '') portals.push({ name: name.trim(), url });
	}
	return portals;
}

const RANK: Record<ContainerState, number> = {
	crashed: 0,
	unknown: 1,
	starting: 1,
	running: 2,
	created: 2,
	exited: 2
};

/** Attention first (crashed, then starting), then by service name. */
export function sortContainers(containers: readonly Container[]): Container[] {
	return [...containers].sort(
		(a, b) => RANK[a.state] - RANK[b.state] || a.service.localeCompare(b.service)
	);
}

/**
 * One `app.query` entry in detail, or `null` when it lacks what the list needs too. Workloads
 * that are missing or unreadable read as none (a stopped app has none).
 */
export function parseAppDetail(raw: unknown, hostname: string): AppDetail | null {
	const app = parseApp(raw);
	if (!app || !isObj(raw)) return null;
	const workloads = isObj(raw.active_workloads) ? raw.active_workloads : {};
	const details = Array.isArray(workloads.container_details) ? workloads.container_details : [];
	const description =
		isObj(raw.metadata) && isStr(raw.metadata.description) ? raw.metadata.description.trim() : '';
	return {
		app,
		description: description !== '' ? description : null,
		portals: parsePortals(raw.portals, hostname),
		containers: sortContainers(
			details.map(parseContainer).filter((c): c is Container => c !== null)
		),
		ports: parsePorts(workloads.used_ports)
	};
}

/** The folders of all containers, once each (by both paths), NAS paths in order. */
export function appMounts(containers: readonly Container[]): Mount[] {
	const seen = new Map<string, Mount>();
	for (const c of containers) {
		for (const m of c.mounts) {
			const key = `${m.source}\n${m.destination}`;
			const known = seen.get(key);
			// Read-only only if every container mounts it read-only.
			if (known) known.readOnly &&= m.readOnly;
			else seen.set(key, { ...m });
		}
	}
	return [...seen.values()].sort(
		(a, b) => a.source.localeCompare(b.source) || a.destination.localeCompare(b.destination)
	);
}

/** How a container state reads: words first, the colour only repeats them. */
export function containerStateLabel(state: ContainerState): {
	label: string;
	tone: Tone;
	dot: boolean;
} {
	switch (state) {
		case 'running':
			return { label: 'Running', tone: 'success', dot: true };
		case 'starting':
			return { label: 'Starting…', tone: 'neutral', dot: true };
		case 'created':
			return { label: 'Not started', tone: 'neutral', dot: false };
		case 'exited':
			return { label: 'Stopped', tone: 'neutral', dot: false };
		case 'crashed':
			return { label: 'Crashed', tone: 'destructive', dot: false };
		case 'unknown':
			return { label: 'Unknown', tone: 'neutral', dot: false };
	}
}

/** `30013 → 8096/tcp`; several host ports are comma-separated, none reads "not published". */
export function portLabel(port: Port): string {
	const target = `${port.containerPort}/${port.protocol}`;
	return port.hostPorts.length > 0 ? `${port.hostPorts.join(', ')} → ${target}` : target;
}
