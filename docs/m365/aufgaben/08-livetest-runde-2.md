# 08 – Live-Test, Runde 2 (2026-11-24)

| Tool | Graph-Antwort | Korrektur |
|---|---|---|
| `onenote_seiten`, `onenote_seite_lesen` | `Could not find a property named 'webUrl' on type 'microsoft.graph.onenotePage'` | `webUrl` raus aus beiden `$select` (`onenote_tools.rs` ~266 und ~294); Link aus `links.oneNoteWebUrl.href`, wie bei Notizbüchern und Abschnitten |
| `teams_reagieren` | `Unicode 'heart' in the payload is not supported` | `reactionType` **muss das Unicode-Emoji sein**. `reaktion_normalisieren()` arbeitet bisher falsch herum und muss umgedreht werden: Wörter → Emoji (`like`→👍, `heart`/`love`→❤️, `laugh`→😆, `surprised`→😮, `sad`→😢, `angry`→😡). Ein Emoji bleibt, wie es ist. Leer oder unbekannt → klare Meldung ohne Anfrage. Gilt für Chat und Kanal, für set und unset |
| `teams_chat_anlegen` | `Duplicate chat members`, wenn als Teilnehmer nur der User selbst genannt ist | Löst ein Teilnehmer auf den eigenen User auf: Meldung „Du kannst keinen Chat nur mit dir selbst anlegen – nenne mindestens eine andere Person.“, **ohne** POST. Der eigene User wird nie doppelt gebunden |

## Attrappe

Die Attrappe antwortet wie Graph:
- 400 bei `webUrl` in `$select` auf pages
- 400 bei setReaction/unsetReaction, wenn `reactionType` kein Emoji ist
- 400 bei doppelten Mitgliedern

Zu jeder Korrektur kommt ein Test, der mit dem alten Code rot wäre.

## Sonst

- Nur diese drei Fehler beheben, sonst nichts ändern.
- Prüfbefehl grün: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
- Nicht committen.
