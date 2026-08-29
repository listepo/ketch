/**
 * Join a site-relative path onto the configured base.
 *
 * `BASE_URL` already carries a trailing slash, so interpolating one in by
 * hand produces `/ketch//docs/` — which works in a browser and looks like a
 * mistake in every crawler, canonical URL and shared link.
 */
export function href(pathname = ""): string {
  return `${import.meta.env.BASE_URL}/${pathname}`.replace(/\/{2,}/g, "/");
}
