# Current Messenger Ecosystem Patterns — 2026-09-23

Research notes supporting [`MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md`](../process/MESSENGER_BASELINE_AND_ECOSYSTEM_QUEUE.md). These are references, not protocol authority.

## Telegram expressive media

- Telegram documents an open sticker platform where users/artists can create and share sets: https://core.telegram.org/stickers
- Static stickers use PNG/WebP; animated stickers use a constrained Lottie/TGS format; video stickers use WebM/VP9. The important design lesson is bounded formats plus small looped artwork, not necessarily copying Telegram's exact codecs.
- Telegram's in-app sticker editor makes packs shareable from a sent sticker, and associates stickers with emoji for suggestions: https://telegram.org/blog/sticker-maker
- Import APIs support bringing externally created packs into the system: https://core.telegram.org/import-stickers

## Telegram group / broadcast / community separation

- Telegram distinguishes broadcast channels from interactive groups and supports forums/topics: https://telegram.org/faq and https://core.telegram.org/api/forum
- In July 2026 Telegram added Communities that link several groups, channels and bots while preserving the contained chats as distinct surfaces: https://telegram.org/blog/communities-editor-invisible-messages
- This is useful evidence that broadcast, interactive room and umbrella/community semantics should be explicit rather than one overloaded room mode.

## Discord

- Discord roles/permissions support server-wide roles plus channel-specific overrides: https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions
- Forum channels are persistent organized discussions distinct from transient threads: https://support.discord.com/hc/en-us/articles/6208479917079-Forum-Channels-FAQ
- Application commands provide first-class app/bot interaction surfaces: https://docs.discord.com/developers/docs/interactions/slash-commands

## Matrix

- Matrix Spaces group multiple independent rooms and are explicitly compared to workspaces/servers: https://www.matrix.org/docs/chat_basics/public-rooms/
- Matrix's event/room extensibility and application-service model are useful interoperability references, though OM deliberately does not adopt Matrix's homeserver authority model: https://spec.matrix.org/latest/ and https://spec.matrix.org/latest/application-service-api/

## Zulip

- Zulip makes persistent topics the primary organization mechanism for concurrent discussions inside a channel: https://zulip.com/help/introduction-to-topics
- Topic follow/mute semantics show how high-volume spaces can stay usable: https://zulip.com/help/follow-a-topic

## WhatsApp Communities

- WhatsApp Communities provide an umbrella plus an admin announcement group: https://faq.whatsapp.com/582420703681043

## Creator commerce

- Telegram Stars are a full platform commerce surface for digital goods, bots, paid media and creator revenue, with purchasing/withdrawal flows and regional limitations: https://core.telegram.org/api/stars
- OM should learn from the user experience without inheriting the operator role: a generic entitlement boundary can be interoperable while payment settlement, merchant/marketplace operation and regional compliance remain external optional roles.

## Legal / licensing boundary

- Apache-2.0 includes an AS-IS warranty disclaimer and limitation of liability, subject to applicable law: https://www.apache.org/licenses/LICENSE-2.0
- MPL-2.0 likewise contains warranty and liability limitations and allows a downstream distributor to offer extra support/warranty only on its own behalf: https://www.mozilla.org/MPL/2.0/
- AGPL-3.0 includes no-warranty/limitation-of-liability provisions and permits a downstream operator to assume additional obligations on its own behalf: https://www.gnu.org/licenses/agpl-3.0.html

The project should use these standard legal mechanisms and role separation rather than inventing a “no business/no liability” pseudo-open license.

## 06:15 architecture-hardening observations

Official/current comparative references sharpened the design:

- Matrix documents that joining a Space does **not** automatically join all contained rooms, and its restricted join rules can use membership in another room as an admission condition. OM adopts the separation of container membership from child membership without adopting homeserver authority: https://www.matrix.org/docs/chat_basics/public-rooms/ and https://spec.matrix.org/latest/client-server-api/
- Telegram's July 2026 Communities link independent groups, channels and bots, including visible/hidden child chats, supporting OM's decision that a Community is a container rather than one overloaded chat: https://telegram.org/blog/communities-editor-invisible-messages
- Discord documents role permissions plus server/category/channel overrides. OM takes the useful granularity but deliberately avoids making a multi-layer allow/deny override lattice or creator superuser a protocol necessity: https://support.discord.com/hc/en-us/articles/206029707-Setting-Up-Permissions-FAQ
- Zulip's topic follow/mute model supports keeping attention/notification state local to the user even when Topic organization is shared: https://zulip.com/help/follow-a-topic and https://zulip.com/help/mute-a-topic
- Telegram channels demonstrate why public one-to-many publication has different semantics from an interactive group. OM makes that separation stronger by using a signed Publication object rather than overloading private MLS Room semantics: https://core.telegram.org/api/channel
