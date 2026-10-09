--------------------------- MODULE WebcamRematch ---------------------------
EXTENDS Naturals, Sequences

CONSTANTS MaxEpoch, MaxRevision, Fixed

\* Two connected Commander seats, ordered Alice then Bob. Epoch is ghost state
\* for pass requests and the server-owned seat generation in the fixed version.
VARIABLES epoch, started, revision, active, pendingPass, stalePassAccepted,
          life, cachedSeat, pendingSeat, resetPending

vars == <<epoch, started, revision, active, pendingPass, stalePassAccepted,
          life, cachedSeat, pendingSeat, resetPending>>

Init ==
    /\ epoch = 0
    /\ started = FALSE
    /\ revision = 0
    /\ active = 0
    /\ pendingPass = <<>>
    /\ stalePassAccepted = FALSE
    /\ life = 40
    /\ cachedSeat = [epoch |-> 0, life |-> 40]
    /\ pendingSeat = <<>>
    /\ resetPending = FALSE

\* Room::start_game -> reorder -> reconcile_turn -> turns::pass.
Start ==
    /\ ~started
    /\ revision < MaxRevision
    /\ started' = TRUE
    /\ revision' = revision + 1
    /\ active' = 1
    /\ UNCHANGED <<epoch, pendingPass, stalePassAccepted, life, cachedSeat,
                    pendingSeat, resetPending>>

\* A browser request can arrive later than another player's rematch/start.
CapturePass ==
    /\ started
    /\ revision < MaxRevision
    /\ pendingPass = <<>>
    /\ pendingPass' = <<[epoch |-> epoch, revision |-> revision]>>
    /\ UNCHANGED <<epoch, started, revision, active, stalePassAccepted,
                    life, cachedSeat, pendingSeat, resetPending>>

\* RoomMsg::PassTurn checks revision and active player, not game generation.
DeliverPass ==
    /\ pendingPass # <<>>
    /\ LET request == Head(pendingPass)
           accepted == started /\ request.revision = revision
       IN /\ revision' = IF accepted THEN revision + 1 ELSE revision
          /\ active' = IF accepted THEN 3 - active ELSE active
          /\ stalePassAccepted' = (stalePassAccepted \/
                 (accepted /\ request.epoch # epoch))
    /\ pendingPass' = <<>>
    /\ UNCHANGED <<epoch, started, life, cachedSeat, pendingSeat, resetPending>>

\* Room::rematch resets turns but advances their revision in the fixed version.
\* It also resets seats and queues SeatReset. Writes succeed and are atomic here.
Rematch ==
    /\ epoch < MaxEpoch
    /\ ~Fixed \/ revision < MaxRevision
    /\ ~resetPending
    /\ epoch' = epoch + 1
    /\ started' = FALSE
    /\ revision' = IF Fixed THEN revision + 1 ELSE 0
    /\ active' = 0
    /\ life' = 40
    /\ resetPending' = TRUE
    /\ UNCHANGED <<pendingPass, stalePassAccepted, cachedSeat, pendingSeat>>

\* An acknowledged update_status(life=17) before a rematch. Atomic shortcut
\* for a completed handler, never concurrent with another handler on Bob.
Damage ==
    /\ epoch = 0
    /\ life = 40
    /\ pendingSeat = <<>>
    /\ ~resetPending
    /\ life' = 17
    /\ cachedSeat' = [epoch |-> epoch, life |-> 17]
    /\ UNCHANGED <<epoch, started, revision, active, pendingPass,
                    stalePassAccepted, pendingSeat, resetPending>>

\* Bob begins a camera-only update using Channel.participant. The handler's
\* remember_seat can queue behind Alice's rematch. The biased select prevents
\* starting a new handler with SeatReset pending, but cannot preempt one.
PrepareSeat ==
    /\ pendingSeat = <<>>
    /\ ~resetPending
    /\ pendingSeat' = <<cachedSeat>>
    /\ UNCHANGED <<epoch, started, revision, active, pendingPass,
                    stalePassAccepted, life, cachedSeat, resetPending>>

\* Room::remember_seat checks the connection (always current here). The fixed
\* implementation also rejects a snapshot whose generation predates a rematch.
RememberSeat ==
    /\ pendingSeat # <<>>
    /\ life' = IF ~Fixed \/ Head(pendingSeat).epoch = epoch
               THEN Head(pendingSeat).life ELSE life
    /\ pendingSeat' = <<>>
    /\ UNCHANGED <<epoch, started, revision, active, pendingPass,
                    stalePassAccepted, cachedSeat, resetPending>>

\* Channel::handle_conn_event updates only its cache/presence, not saved life.
ReceiveReset ==
    /\ resetPending
    /\ pendingSeat = <<>>
    /\ cachedSeat' = [epoch |-> epoch, life |-> 40]
    /\ resetPending' = FALSE
    /\ UNCHANGED <<epoch, started, revision, active, pendingPass,
                    stalePassAccepted, life, pendingSeat>>

Next == Start \/ CapturePass \/ DeliverPass \/ Rematch \/ Damage \/
        PrepareSeat \/ RememberSeat \/ ReceiveReset

TypeOK ==
    /\ epoch \in 0..MaxEpoch
    /\ started \in BOOLEAN
    /\ revision \in 0..MaxRevision
    /\ active \in 0..2
    /\ pendingPass \in {<<>>} \cup
          {<<r>> : r \in [epoch : 0..MaxEpoch, revision : 0..MaxRevision]}
    /\ stalePassAccepted \in BOOLEAN
    /\ life \in {17, 40}
    /\ cachedSeat \in [epoch : 0..MaxEpoch, life : {17, 40}]
    /\ pendingSeat \in {<<>>} \cup
          {<<s>> : s \in [epoch : 0..MaxEpoch, life : {17, 40}]}
    /\ resetPending \in BOOLEAN

ActiveMatchesPhase == IF started THEN active \in {1, 2} ELSE active = 0
NoCrossGamePass == ~stalePassAccepted
\* No action after epoch 0 requests a life change, so a camera update must
\* never bring the previous game's life back.
ResetLifePreserved == epoch > 0 => life = 40
=============================================================================
