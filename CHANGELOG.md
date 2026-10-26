# Changelog

All notable changes to The Gathering are documented here.

Versions follow [Semantic Versioning](https://semver.org/).
This file is generated from `git log` using [git-cliff](https://git-cliff.org).
See [docs/releasing.md](docs/releasing.md) for maintainer instructions.

---

## [1.0.0] - 2026-10-26

### Bug Fixes

- **webcam:** Ride out connection drops without duplicate seats ([`5e6c6fa0`](../../commit/5e6c6fa0226c56234b510d6d6123de15aab919f9))
- **webcam:** Isolate rematches from stale turn and seat updates ([`ec9db0a9`](../../commit/ec9db0a997cb6ad890427e81ee020dac27981494))
- **discord:** Distinguish newgame database failures from invalid options ([`cfeb559b`](../../commit/cfeb559b109d349b24e46f90b9514f9c8d1c760c))
- **webcam:** Clear stale turn errors after successful actions ([`23287a5c`](../../commit/23287a5c215419ff8117fe34115454b8bb9b31bb))
- Align game result notes field ([`6b601784`](../../commit/6b601784f37ac94ff5c28a5fd9392b8218cd4467))
- **audit:** Retain only committed data mutations ([`7d5041d1`](../../commit/7d5041d1cfa601c67d99e4eaf75e0429963cd49e))
- **webcam-table:** Release idle pointer focus for hotkeys ([`0aefd899`](../../commit/0aefd899b3b1134f2d1e7b1c52f3d48f37b78db2))
- **decks:** Preserve local settings during hosted sync ([`324f5926`](../../commit/324f5926d1ee76659992bd798f241800cfe037a0))
- Restrict hosted deck sync to Commander format ([`4fa54450`](../../commit/4fa54450da2cbd4bec27ef13273dfa8c7eb343e2))
- Use profile owner in rivalry captions ([`dfaa5b39`](../../commit/dfaa5b390fde567456d61635502efcbf768619eb))
- **discord:** Post the /newgame queue inside its thread ([`66b20e37`](../../commit/66b20e37c60128fff7796b7cfa7fe35628bce685))
- **discord:** Keep queues that started their thread on the channel message ([`d0fda0a0`](../../commit/d0fda0a0212e6e7783e25771a39e7547d158f023))
- **webcam-table:** Reachable life controls and Leave table on phones ([`cd7e4f2f`](../../commit/cd7e4f2fb19bd344eaf5c52b9d75b08ab03b6928))
- **webcam-table:** Keep the virtual board across camera switches ([`17e1fe8e`](../../commit/17e1fe8ee9cc06a7745bd72b3ec0938703be8944))
- **catalog:** Sync at boot until printings are imported ([`27e6681a`](../../commit/27e6681a9c8f37612123aff9685511c38de45cbf))
- **webcam-table:** Keep 2HG team tiles full height in the camera rail ([`c4ea357b`](../../commit/c4ea357bba9b35182c6f41b04d1f681c146b0074))
- **webcam-table:** Offer the deck chooser after the start until a deck is picked ([`fe7c467e`](../../commit/fe7c467e292700c9298352facf1890f952a7cac8))
- **deps:** Distrust the compromised eMudhra intermediates (webpki-roots 1.0.10, rustls-webpki 0.103.16) ([`41d5b911`](../../commit/41d5b911c20df5b76297f879263c3b3a5590ae35))
- **games:** Keep game seat cards a consistent height ([`2a8328e7`](../../commit/2a8328e73f60577747181d64d7a46616235e816b))
- Record AI ruling spend when the asker disconnects ([`293e42dc`](../../commit/293e42dc7f85b31bd2be342677522f1540fd3aec))
- **security:** Limit password login per username and globally ([`b725d99d`](../../commit/b725d99de985e4274a19340e660d4df49ebf5775))
- Mark the session cookie Secure behind an https public URL ([`66903dbd`](../../commit/66903dbd455b4c89df891c7a54c4ea4a07ae521b))
- **auth:** Keep sign-in returnTo redirects on this site ([`df8683ce`](../../commit/df8683cecd2a7d50f4b21cf3d44d65de2dd01e0d))
- **webcam:** Cap the tables one user can be connected to at once ([`c02f0f76`](../../commit/c02f0f76aaa4ecaf553fe78d25b09ee71616b6c6))
- **webcam:** Reapply a seat's private reveal at the SFU when it rejoins ([`c6eebd63`](../../commit/c6eebd63bca90fce2b71582d19d722896be62f4e))
- Cap stored card-ID corrections per member and in total ([`db5744e2`](../../commit/db5744e2c0d1830af7fcfebeaa6abef35515000a))
- **deck-scripts:** Keep library functions from outrunning the Lua sandbox limits ([`3190179b`](../../commit/3190179b241e6df63f6d8d84ad08e1c59ef959cc))
- Close a user's table sockets when an administrator changes their account ([`de4fe90f`](../../commit/de4fe90f7a37d174fbb65a8a47f7dc414c07ebe2))
- **sfu:** Bound trickled ICE candidates and drop seats that never offer ([`544ce60b`](../../commit/544ce60b0b29bbf2a28eefdba85ac0dedfc406e4))
- Honor proxy headers only from trusted proxy peers ([`6462fbc6`](../../commit/6462fbc6b848d48233fef8ef95035a43c2a93234))
- **ai:** Stop sending tool_choice to OpenRouter ([`15eefc76`](../../commit/15eefc76f52957e792e930af8c87a93e877844be))
- **deps:** Patch rustls, tokio, vite-plus/vitest, undici, zod and source-map-js ([`029444a2`](../../commit/029444a23c589cb2502228d58d40fd12e53b8c6d))

### Build

- Install Rust with the minimal rustup profile ([`ca483702`](../../commit/ca4837027213ecd78917d1e345e1d70ac181ddc1))
- Upgrade aube to 2.8.1 ([`6f6e6913`](../../commit/6f6e69139009fa4c417f42807552d2379e8b71c4))

### CI

- **release:** Stop caching node_modules ([`6714fae3`](../../commit/6714fae3bc4a8b864d65262b06bff6b996fd668b))

### Chores

- Track the Phoenix-compatibility removal plan in Backlog ([`ad8b3b50`](../../commit/ad8b3b500c6e0b7857855efff66397f8800f5d1d))
- **webcam:** Log table socket ids and why seats end ([`7bedd81f`](../../commit/7bedd81f03c006a2f8b137119e34487a4a92b3dc))
- Close the Phoenix-compatibility removal plan ([`3a291697`](../../commit/3a291697ac6fa01b46f485ebf9f9dcf352a3628c))
- Ignore generated dependency files ([`295ab99c`](../../commit/295ab99c3c4b4a293607768264a0fe3d42d2fb33))
- Add git-cliff changelog and release tooling ([`c4d16a40`](../../commit/c4d16a40f683fd43e1d24991caa7c5d3c6adbb6f))

### Documentation

- Present the README as a webcam-first product ([`a6a542ba`](../../commit/a6a542baa66d78d94ffd2c9897ecc4af6f748af6))

### Features

- **admin:** Add user audit history and live server logs ([`b2ae0c4f`](../../commit/b2ae0c4fbd5c705aa203671114bcf4890014ffba))
- **webcam:** Let hosts and admins remove table participants ([`1cea34d0`](../../commit/1cea34d0ed480e04c360982a7bc25553dfc10fc6))
- **audit:** Display changed fields as a unified diff ([`917a9068`](../../commit/917a90689e9ed3b62b16f1c3c3b1cffeb2deb768))
- **games:** Track kill victims with optional totals ([`bf5dfd45`](../../commit/bf5dfd452a95b08857579064a4672ababfc5885c))
- **stats:** Add kill rivalries and game drilldowns ([`e9b86d3e`](../../commit/e9b86d3eeb59f6bfcb1c5c90297a571d9111b974))
- **webcam-table:** Record kills and MVP cards after games ([`7f4a3b87`](../../commit/7f4a3b876ffd58b1a72b29f76ec0b4e2326a3031))
- Let any member export game history from the games page menu ([`ae708318`](../../commit/ae7083185a30e0e64d48305d1daf51c6146e1410))
- **discord:** Gather /newgame players in a thread and add /close ([`538f1c3b`](../../commit/538f1c3b1bb46e8b3cabbd9bcde9335958b6be8a))
- **discord:** Date game threads and reuse them for /newgame ([`f701dbb6`](../../commit/f701dbb6bfb3a96223a085e6f0aa7541ee794d4f))
- **discord:** Mention an opt-in role in each game thread ([`f2f8aa31`](../../commit/f2f8aa3153f7672c0b2da8bca638dab4c04b0797))
- **discord:** Add /alerts to toggle the new game role ([`214d9023`](../../commit/214d9023dca0b03ba6840901c80e68b3a33fc467))
- **cardid:** Install recognition bundles from GitHub releases ([`f81bacfb`](../../commit/f81bacfbc3328b84c6dbc0ac6a9c8db1c4db62bb))
- **webcam-table:** Warn when a browser captures card crops as black ([`940f2dd3`](../../commit/940f2dd34158d587cb49f914572054475c9b8e79))
- **webcam-table:** Let hosts and admins seat spectators in the lobby ([`d6f94246`](../../commit/d6f94246bd7c905ef120afcf0c7d2118d87bd61a))
- **catalog:** Serve printings, details, and rulings from bulk data ([`1540b071`](../../commit/1540b07147b3e7e47da5c4859e5e59daabbac137))
- **decks:** Let members weight the deck chooser with Lua scripts ([`ddd80318`](../../commit/ddd80318a6c2617ac41a321bba73bf01448cd287))
- **webcam-table:** Board-first table layout on phones ([`bbbda980`](../../commit/bbbda98005639884300a8c5ea5a07f50f71a3274))
- **webcam-table:** Play on a virtual board instead of a camera ([`67647584`](../../commit/67647584107618563e3087a29c77d7ae7a942071))
- **admin:** Sync the catalog from the admin catalog page ([`8e290e07`](../../commit/8e290e070a09a74b15b356cba780ddc3e0cfeb2d))
- **webcam-table:** London mulligan option on the virtual board ([`6497f37c`](../../commit/6497f37c9301045adfe3ab19a29797cff1cf389a))
- **games:** Show turns and game time on game cards ([`726140eb`](../../commit/726140eb35bc95f9e86eb49448b9160e0d901cf6))
- Add Duel Commander and unrecorded Standard, Pauper, and Canadian Highlander table modes ([`262d4660`](../../commit/262d46607c14685f093f4bd90379cb92bfaa4809))
- **webcam:** Log who passed or un-passed the turn ([`dda9fcd9`](../../commit/dda9fcd93bd8380d4181b73799096957e391cda7))
- **webcam:** Focus the card picker search when it opens ([`a5485b81`](../../commit/a5485b81bccb235ae1a4ee67d198cf797663d7ec))
- Ask an AI judge for rulings at the table ([`23ecbc15`](../../commit/23ecbc1568495d6443edac46f7e27a77dac6a9d6))
- Make table cards opt-in for rulings and exempt admins from the spend limit ([`632a0729`](../../commit/632a07299d013a790b434036d17d13b0eb0edbfe))
- Cache ruling prompts on OpenRouter and record cache usage ([`25decee6`](../../commit/25decee6ed3a0f2ada4720d92bd59b696ca100cf))
- Require a tool lookup on the first ruling round ([`d46f5036`](../../commit/d46f5036bf88cd64f468c5bdfb0665a5ee6701ca))
- Sign AI ruling turns and tighten the topic rule ([`98079761`](../../commit/980797614345528d61dcfa5e561fc1023f72f114))
- **table:** Add last-counter and follow-turn hotkeys ([`5f8b1538`](../../commit/5f8b1538987e0ac944d0f464382c7fdd07f32e34))

### Refactoring

- **web:** Keep sessions in a typed private cookie and drop Plug.Crypto emulation ([`59a23c27`](../../commit/59a23c27e555ba709f52a25a05a45e24887b3c96))
- **config:** Rename SECRET_KEY_BASE and PHX_* to THE_GATHERING_SECRET_KEY and THE_GATHERING_PUBLIC_URL ([`51931376`](../../commit/519313767f39514dd5767390fa76b37bbd92073b))
- **api:** Replace Ecto-shaped validation errors with ValidationError ([`6e7a69bc`](../../commit/6e7a69bc45e8c295e0917297c26b34ad5b885392))
- **api:** Typed request bodies for account and admin endpoints ([`7585b1ff`](../../commit/7585b1ff73b91617c9932f9712598dce67dfba20))
- **games:** Typed inputs for players, decks, and games ([`628deb5f`](../../commit/628deb5fcc3acaa21e59ff7b15e0b875c36f6a8d))
- **web:** Typed extractors everywhere, axum 405s, and tower-http request logs ([`3ba07713`](../../commit/3ba07713e6530e54b52d903dc818a29eed112920))
- **webcam:** Replace Phoenix Channels with Socket.IO for the webcam table ([`5f9cea28`](../../commit/5f9cea28ac8ab6f5db2be6e124d628259d3efc3f))
- Typed timestamps and docs without the Elixir framing ([`2651d0f5`](../../commit/2651d0f5efdbca41f22cfa996c19c10cb1154411))

