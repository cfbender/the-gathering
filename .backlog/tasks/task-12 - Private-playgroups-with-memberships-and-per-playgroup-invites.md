---
id: TASK-12
title: Private playgroups with memberships and per-playgroup invites
status: Done
assignee:
  - '@cfbender-pdq'
created_date: '2026-11-07T21:06:00Z'
updated_date: '2026-11-07T21:51:00Z'
labels: []
dependencies: []
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Playgroups (TASK-11) are visible to every member of the server, so a server that hosts several circles of players cannot keep each circle's games, decks, and players to its own people. Members need to see only the playgroups they belong to while still belonging to several, administrators need to manage who belongs where, and new members need a way to join a specific playgroup (the sign-up invitation is server-wide today). The Discord bot records a guild's games into the playgroup that claims it, so multiple servers already route correctly as long as DISCORD_GUILD_ID is left unset.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 GET /api/playgroups lists only playgroups the signed-in member belongs to, and switching to any other playgroup fails with a 422 that does not reveal its existence
- [x] #2 Every existing account becomes a member of the default playgroup when the migration runs, so an upgraded server behaves as before
- [x] #3 Administrators list every playgroup with its members, add and remove members (admin + sudo), and an administrator who creates a playgroup is its first member
- [x] #4 Each playgroup has its own reusable sign-up invitation; signing up through it (or opening it while signed in) joins that playgroup, and open registration without an invitation joins the default playgroup
- [x] #5 An account with no playgroups sees an explanatory empty state instead of lists and errors, and the switcher names the situation
- [x] #6 Linked players follow memberships: signing in with Discord resolves a player in each playgroup the account belongs to, and a display-name change renames every linked player
- [x] #7 Integration tests cover visibility, switching, membership management, invites, and the migration backfill; vitest covers the switcher and admin page states
- [x] #8 README documents memberships, per-playgroup invites, and how the Discord bot serves several servers
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Extend the unshipped create_playgroups migration: playgroup_members (playgroup_id, user_id, PK, cascades), backfill every user into the default playgroup; move server_settings.registration_invite_hash to playgroups.registration_invite_hash (default playgroup keeps the current digest). Regenerate schema.sql and .sqlx; recreate the dev DB.
2. Backend: User.playgroup_id becomes Option (users.playgroup_id when a member, else lowest membership); User::playgroup() yields a 403 for accounts without one; set_playgroup checks membership. playgroups.rs gains memberships (list for user, add, remove, members of a playgroup), per-playgroup invite rotate/validate, and create joins the creator. Discord sign-in joins the invite's playgroup (or the default without an invite) and resolves a player per membership; rename_linked_player renames all linked players; delete_user removes memberships.
3. API: GET /api/playgroups (memberships only); sudo admin GET /api/admin/playgroups with members, POST/DELETE members, GET/POST /api/admin/playgroups/{id}/registration-invite replacing /api/admin/registration-invite; public registration-invite endpoints validate across playgroups and join a signed-in member.
4. Frontend: switcher shows memberships and a no-playgroup state; root layout renders an empty state for accounts without a playgroup; admin playgroups page lists all playgroups with members, add/remove, and the per-playgroup invite; invite page joins when signed in.
5. Tests (integration + vitest), README, precommit, portal verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Verified through the review portal (dev DB rebuilt on the final migration): admin Playgroups page with member chips, add-member select, confirm-remove dialog, per-playgroup invite link create/rotate; signed-in invite join lands on 'Welcome to <playgroup>' and switches the session; header switcher lists memberships; no-playgroup empty state renders for an account with no memberships and scoped APIs return 403 no_playgroup. Header fix found during verification: brand is shrink-0, the switcher shrinks with truncation, and the desktop nav breakpoint moved from lg to xl because the header overflowed at 1024-1279 with the switcher present. Decisions: only Discord-identified accounts get a player automatically on join (password accounts are linked by an admin); a playgroup deletes once it has no games and no decks, removing its players and memberships; admins see every playgroup on the admin page but must be members to view one; a signed-in user who opens an invite is added and switched immediately. Discord: no bot change; leave DISCORD_GUILD_ID unset for several servers and claim each guild on its playgroup (DISCORD_NEWGAME_ROLE_ID is still a single role). mise run precommit green: 691 integration + 92 unit Rust tests, 576 vitest, vp check, build.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Playgroups are private: playgroup_members backfilled with every user into Main; GET /api/playgroups lists memberships only and switching requires membership; admin endpoints list all playgroups with members, add/remove members, and rotate a per-playgroup invite (replacing the server-wide one); invite joins the invited playgroup for new Discord sign-ups and for signed-in users; accounts without memberships get a 403 no_playgroup and an empty state. Frontend: admin page with members and invite per playgroup, switcher/no-playgroup states, invite page join state, header layout fix. README and .env.example document memberships, invites, and multi-server Discord setup. Verified by integration + vitest suites, precommit, and portal screenshots.
<!-- SECTION:FINAL_SUMMARY:END -->
