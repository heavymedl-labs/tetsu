// Requests from an island to Tetsu's own JSON endpoints, which every island makes the same way.

/** A refusal the server gave, in words a page may show, with its HTTP status: 401 once the session has ended. */
export class Refused extends Error {
  readonly status: number;

  constructor(message: string, status = 400) {
    super(message);
    this.status = status;
  }
}

/** Posts `body` to `path` as JSON, or gets it when there is no body, and reads the JSON answer. Any answer but a
 * success is the server's refusal, in its words if it gave some: a proxy's or rate limiter's may not be JSON. */
export async function call(
  path: string,
  signal?: AbortSignal,
  body?: object,
): Promise<Record<string, unknown>> {
  const sent: RequestInit =
    body === undefined
      ? {}
      : { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) };
  const response = await fetch(path, { ...sent, signal: signal ?? null });
  const json = (await response.json().catch(() => ({}))) as Record<string, unknown>;
  if (!response.ok) {
    throw new Refused(
      typeof json.error === "string" ? json.error : "this request was refused",
      response.status,
    );
  }
  return json;
}
