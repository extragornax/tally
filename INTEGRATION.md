# Tally — Integration Guide

Self-hosted analytics for **extragornax.fr** sites. Pixel-only tracker. No cookies. No JavaScript. No personal data stored. RGPD-compliant.

Hand this file to any service (or AI agent) that needs to add Tally tracking to a site.

---

## TL;DR

Add **one line** before `</body>`:

```html
<img src="https://tally.extragornax.fr/t/SITE_ID" alt="" width="1" height="1" style="position:absolute;opacity:0" referrerpolicy="no-referrer-when-downgrade" />
```

Replace `SITE_ID` with your site's slug. Done.

---

## Site IDs

Use the slug that matches your subdomain (without `.extragornax.fr`).

| Slug | Site |
|------|------|
| `gpx` | gpx.extragornax.fr |
| `kahoot` | kahoot.extragornax.fr |
| `brm` | brm.extragornax.fr |
| `meteo` | meteo.extragornax.fr |
| `ravito` | ravito.extragornax.fr |
| `strava` | strava.extragornax.fr |
| `countdown` | countdown.extragornax.fr |
| `plik` | plik.extragornax.fr |
| `trace` | trace.extragornax.fr |
| `cyclo` | cyclo.extragornax.fr |
| `roulette` | roulette.extragornax.fr |
| `homepage` | extragornax.fr |
| `dotwatcher` | dotwatcher.extragornax.fr |
| `mayojaune` | mayojaune.extragornax.fr |

New site? Ask admin to add slug to `SITES` env var on Tally server before integrating.

---

## How it works

1. Browser loads `<img src=".../t/SITE_ID">`.
2. Tally server logs: timestamp, site, referrer domain + path, country (from IP), device class. **No IP, no UA string, no cookie stored.**
3. Server returns 43-byte transparent GIF.
4. Visitor hash = SHA-256(IP + site + date) truncated to 16 chars. Salt rotates daily → IP cannot be reconstructed, visitor cannot be tracked across days.
5. Bots (UA contains `bot`/`crawl`/`spider`/`slurp`/`feed`) skipped entirely.

---

## Integration per stack

### Static HTML

```html
<!DOCTYPE html>
<html>
  <body>
    <!-- ... your content ... -->
    <img src="https://tally.extragornax.fr/t/SITE_ID" alt="" width="1" height="1" style="position:absolute;opacity:0" />
  </body>
</html>
```

### Server-rendered template (Jinja / Tera / Handlebars / Twig)

Inject in your base layout, before `</body>`:

```html
{# base.html.j2 #}
<img src="https://tally.extragornax.fr/t/{{ tally_site_id }}" alt="" width="1" height="1" style="position:absolute;opacity:0" />
```

Pass `tally_site_id` from config.

### Rust / Axum (askama, maud, etc.)

Embed in your layout template. Read site ID from env at startup:

```rust
let tally_site_id = std::env::var("TALLY_SITE_ID").unwrap_or_default();
```

### React / Next.js / Vue / Svelte

Put in root layout, not per-page component (avoid double-counting on client routing). For SPAs **track only initial load** — that's the design. If you need route-change tracking, append a query string per route change:

```jsx
// Next.js app/layout.tsx
export default function RootLayout({ children }) {
  return (
    <html>
      <body>
        {children}
        <img
          src="https://tally.extragornax.fr/t/SITE_ID"
          alt=""
          width={1}
          height={1}
          style={{ position: "absolute", opacity: 0 }}
          referrerPolicy="no-referrer-when-downgrade"
        />
      </body>
    </html>
  );
}
```

### Astro / 11ty / Hugo / Jekyll

Drop the snippet in your default layout partial.

---

## CSP-compatible

If you serve a strict Content-Security-Policy, add:

```
img-src 'self' https://tally.extragornax.fr;
```

No `script-src` change needed (no JS).

---

## Referrer policy (important)

Default browser referrer behavior is fine. **Do not set `referrerpolicy="no-referrer"`** — Tally needs the `Referer` header to extract the page path and external source domain. Recommended:

```html
referrerpolicy="no-referrer-when-downgrade"
```

Same-origin and HTTPS→HTTPS will send full referrer. Downgrades (HTTPS→HTTP) won't.

---

## What gets recorded

| Field | Source | Stored? |
|-------|--------|---------|
| Timestamp | server clock | yes (UTC, RFC3339) |
| Site ID | URL path | yes |
| Path | `Referer` header | yes (path only, no query string) |
| Referrer domain | `Referer` host | yes |
| Country | IP → MaxMind lookup (in-memory only) | yes (2-char ISO) |
| Device | UA classification (`desktop`/`mobile`/`tablet`/`bot`) | yes |
| Visitor hash | SHA-256(IP+site+date), 16 hex chars | yes |
| Is-unique flag | first hit of (site, day, hash) | yes |
| **IP address** | request | **NO — never written to disk** |
| **User-Agent string** | request | **NO — only the device class** |

---

## What does NOT get recorded

- IP addresses
- User-Agent strings
- Cookies (none set, none read)
- Fingerprints
- Click positions / scroll depth / time-on-page
- Cross-day visitor identity (salt rotates daily)

---

## Testing your integration

After deploying:

```bash
curl -I "https://tally.extragornax.fr/t/SITE_ID" \
  -H "User-Agent: Mozilla/5.0" \
  -H "Referer: https://your-site.example/page"
```

Expected:
```
HTTP/2 200
content-type: image/gif
cache-control: no-store, no-cache, must-revalidate, private
```

Check dashboard (admin only):
```
https://tally.extragornax.fr/?token=ADMIN_TOKEN
```

Hit should appear within 5s (batch flush interval).

---

## Common mistakes

- Putting the pixel in `<head>` — works but `</body>` placement is canonical
- Setting `display:none` — some browsers skip loading; use `opacity:0` + `position:absolute`
- Wrong site ID — server returns 404 GIF, hit not counted
- Setting `referrerpolicy="no-referrer"` — loses page path + traffic source data
- Conditionally rendering the pixel based on cookie consent banners — RGPD-friendly by design, no consent needed

---

## Contact

Issues / new site ID requests → repo owner.
