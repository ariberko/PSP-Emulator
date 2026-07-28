/**
 * Cloud save states.
 *
 * - `GET /save-sync?disc_id=…` — list the caller's states, newest first. Omit
 *   `disc_id` to list everything, for a "resume where you left off" view.
 * - `POST /save-sync` — record an uploaded state. The desktop app uploads the
 *   `.ppst` payload first (via the SDK's file upload) and posts the resulting URL
 *   plus metadata here.
 * - `DELETE /save-sync?id=…` — remove one of the caller's states.
 *
 * Unlike the release manifest, everything here runs as the calling **user**, not
 * the service role. Save states are private, and using elevated access would mean
 * this function had to enforce ownership itself on every path — one missed check
 * away from letting anyone read anyone else's saves. Letting Base44's own access
 * rules scope the rows removes that whole class of bug.
 */

import { createClientFromRequest } from 'npm:@base44/sdk';

/** PPSSPP exposes five save slots. */
const MAX_SLOT = 4;
/** Generous ceiling for a PSP save state; well past any real one. */
const MAX_SIZE_BYTES = 64 * 1024 * 1024;

interface SaveStatePayload {
  disc_id?: unknown;
  game_title?: unknown;
  slot?: unknown;
  file_url?: unknown;
  screenshot_url?: unknown;
  size_bytes?: unknown;
  checksum?: unknown;
  device_name?: unknown;
  captured_at?: unknown;
}

export default async function handler(req: Request): Promise<Response> {
  const base44 = createClientFromRequest(req);

  // Every path needs an identity, so establish it once up front.
  let user: { id: string; email?: string } | null = null;
  try {
    user = await base44.auth.me();
  } catch {
    user = null;
  }
  if (!user) {
    return json({ error: 'Sign in to sync save states' }, 401);
  }

  const url = new URL(req.url);

  switch (req.method) {
    case 'GET':
      return list(base44, url);
    case 'POST':
      return create(base44, req);
    case 'DELETE':
      return remove(base44, url);
    default:
      return json({ error: `${req.method} is not supported` }, 405);
  }
}

async function list(base44: any, url: URL): Promise<Response> {
  const discId = url.searchParams.get('disc_id');
  // Scoped to the caller by Base44's access rules, not by a filter here.
  const filter = discId ? { disc_id: discId } : {};

  try {
    const states = await base44.entities.SaveState.filter(filter, '-captured_at');
    return json({ states, count: states.length });
  } catch (error) {
    return json({ error: `Could not read save states: ${describe(error)}` }, 502);
  }
}

async function create(base44: any, req: Request): Promise<Response> {
  let payload: SaveStatePayload;
  try {
    payload = await req.json();
  } catch {
    return json({ error: 'Body must be JSON' }, 400);
  }

  const parsed = validate(payload);
  if ('error' in parsed) {
    return json({ error: parsed.error }, 400);
  }
  const state = parsed.value;

  try {
    // One row per (disc, slot): a newer save for a slot replaces the old one
    // rather than accumulating, which is how a save slot behaves on hardware.
    const existing = await base44.entities.SaveState.filter({
      disc_id: state.disc_id,
      slot: state.slot,
    });

    if (existing.length > 0) {
      // Identical checksum means the device already has this exact state; say so
      // instead of rewriting the row, so a client can skip re-uploading.
      if (existing[0].checksum === state.checksum) {
        return json({ state: existing[0], unchanged: true });
      }
      const updated = await base44.entities.SaveState.update(existing[0].id, state);
      // Extra rows for this slot mean an earlier concurrent write; collapse them.
      for (const stale of existing.slice(1)) {
        await base44.entities.SaveState.delete(stale.id).catch(() => {});
      }
      return json({ state: updated, replaced: true });
    }

    const created = await base44.entities.SaveState.create(state);
    return json({ state: created, created: true }, 201);
  } catch (error) {
    return json({ error: `Could not save: ${describe(error)}` }, 502);
  }
}

async function remove(base44: any, url: URL): Promise<Response> {
  const id = url.searchParams.get('id');
  if (!id) {
    return json({ error: 'id is required' }, 400);
  }
  try {
    // A row belonging to someone else is not visible to this client, so the
    // delete fails rather than succeeding across accounts.
    await base44.entities.SaveState.delete(id);
    return json({ deleted: true });
  } catch (error) {
    return json({ error: `Could not delete: ${describe(error)}` }, 502);
  }
}

interface ValidState {
  disc_id: string;
  game_title?: string;
  slot: number;
  file_url: string;
  screenshot_url?: string;
  size_bytes?: number;
  checksum: string;
  device_name?: string;
  captured_at: string;
}

/**
 * Checks a posted state before it reaches the entity.
 *
 * The entity's own `required` list would reject missing fields, but not a slot of
 * 99 or a `captured_at` that is not a date — and a bad row is far more annoying to
 * clean up than to refuse.
 */
export function validate(payload: SaveStatePayload): { value: ValidState } | { error: string } {
  const discId = asString(payload.disc_id);
  if (!discId) {
    return { error: 'disc_id is required' };
  }

  const slot = Number(payload.slot);
  if (!Number.isInteger(slot) || slot < 0 || slot > MAX_SLOT) {
    return { error: `slot must be an integer from 0 to ${MAX_SLOT}` };
  }

  const fileUrl = asString(payload.file_url);
  if (!fileUrl) {
    return { error: 'file_url is required — upload the state before recording it' };
  }

  const checksum = asString(payload.checksum);
  if (!checksum) {
    return { error: 'checksum is required' };
  }

  const capturedAt = asString(payload.captured_at);
  if (!capturedAt || Number.isNaN(Date.parse(capturedAt))) {
    return { error: 'captured_at must be an ISO 8601 timestamp' };
  }

  const size = payload.size_bytes === undefined ? undefined : Number(payload.size_bytes);
  if (size !== undefined && (!Number.isFinite(size) || size < 0 || size > MAX_SIZE_BYTES)) {
    return { error: `size_bytes must be between 0 and ${MAX_SIZE_BYTES}` };
  }

  return {
    value: {
      disc_id: discId,
      game_title: asString(payload.game_title),
      slot,
      file_url: fileUrl,
      screenshot_url: asString(payload.screenshot_url),
      size_bytes: size,
      checksum,
      device_name: asString(payload.device_name),
      captured_at: new Date(capturedAt).toISOString(),
    },
  };
}

function asString(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
