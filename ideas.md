# Ideas

## Localisation of the desktop apps

Deferred by the creator (2026-10-01, R11 open decision 7): the desktop apps ship in English only for now. When localisation is wanted, each app keeps its native format (String Catalog, `.resw`, gettext PO) and one Weblate project translates all three (`docs/research-desktop-platforms.md`, section 2, "Strings"). The first step was planned as R11's task D6, "String keys and glossary for three apps":

Three apps with three native string formats (String Catalog, `.resw`, gettext) will translate the same phrases differently unless the keys and terms are agreed once. Research: section 2, "Strings".

Done when a short convention for string keys and an English glossary of ketch's terms (package, source, pin, hold, store, link) are in the desktop docs, and each app's localisation task points at them.

## Linux tray

Deferred by the creator (2026-10-01, R11 open decision 8): the Linux app has no tray and relies on the Background portal. The option kept for later is a StatusNotifierItem through `libayatana-appindicator-glib`, which is GPL-3.0 and so fits only the GPL build (`docs/research-desktop-platforms.md`, sections 1 and 3c).
