# TyuLink Protocol v1

## Discovery and transport

DNS-SD service: `_tyu._udp.local.`. TXT records: `v=1.0`, opaque certificate-derived `id`, and `caps` (capability count). SRV gives the QUIC port. Advertisements are untrusted hints, never authorization. Device names, tickets, certificates and filenames are not advertised. Actual capabilities travel inside authenticated TLS.

QUIC uses Quinn + rustls with **TLS 1.3 only**, ALPN `tyu/1`. Both sides prove possession of their certificate private key. 0-RTT is disabled. Keepalive is 5 seconds; transport idle timeout is 20 seconds. Control/RPC and initial hello have 15-second operation deadlines. QR approval expires at the ticket deadline (120 seconds); discovery approval allows 120 seconds from its request. Long file streams support pause and remain bounded by connection cancellation and quotas.

| Channel | Mapping |
| --- | --- |
| CONTROL | First persistent reliable bidirectional stream; hello/admission then requests and acknowledgements |
| FILES | One reliable bidirectional stream per transfer; FileOffer/Accept, raw bounded chunks, FIN, completion |
| STORAGE | One reliable bidirectional stream per RPC, bounded request/result |
| INPUT | Independent reliable bidirectional RPC streams |
| MEDIA | QUIC DATAGRAM; session/stream identifiers distinguish audio and video |

No unidirectional application streams are accepted. Incoming auxiliary streams are serviced only after admission succeeds. All streams still share connection congestion control; separate streams avoid reliable ordered delivery blocking unrelated streams, not shared bandwidth constraints.

## Binary frame

All integers in the fixed envelope are network-byte-order:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | `TYU1` magic |
| 4 | 1 | Major = 1 |
| 5 | 1 | Minor = 0 |
| 6 | 2 | Explicit message type 1–33 |
| 8 | 4 | Postcard payload length, at most 131072 |
| 12 | length | Postcard `Frame { request: UUID, session: Option<UUID>, message }` |

`Message` variant order and field order are append-only schema commitments. Envelope type must match the decoded variant. Trailing bytes, unknown types, invalid versions and oversized lengths are errors before dispatch. Golden fixtures pin the wire representation. Future incompatible payload changes require a major version; the current decoder accepts only v1.0 framing. The negotiation helper selects v1.0 for a compatible major with a newer advertised minor.

All frames have a request UUID. Responses echo the request and session. Mode, service, media and input operations require a session UUID. File identity is `TransferMetadata.id` and its request UUID, with no display-session dependency. Error responses carry `TyuErrorCode`, not arbitrary diagnostic strings.

## Message inventory

| IDs | Messages |
| --- | --- |
| 1–2 | HELLO, HELLO_ACK (DeviceInfo) |
| 3–6 | PAIR_REQUEST (ticket + nonce), PAIR_CONFIRM, PAIR_REJECT, PAIR_COMPLETE |
| 7–12 | AUTH, CAPABILITIES, HEARTBEAT, PING, PONG, GOODBYE |
| 13–16 | MODE_START, MODE_STOP, SERVICE_START, SERVICE_STOP |
| 17–22 | FILE_OFFER, FILE_ACCEPT, FILE_REJECT, FILE_PROGRESS, FILE_CANCEL, FILE_COMPLETE |
| 23–24 | STORAGE operation and STORAGE_RESULT |
| 25–28 | MEDIA_START, MEDIA_STOP, MEDIA_CONFIG, MEDIA_KEYFRAME_REQUEST |
| 29–32 | INPUT_EVENT, CLIPBOARD, ERROR, ACK |
| 33 | PAIR_NEARBY_REQUEST (random nonce; discovery pairing with host approval) |

STORAGE contains typed List/Stat/Read/Write/CreateDir/Rename/Delete operations. Read/write chunks are at most 65536 bytes; directory results contain at most 256 entries. Pagination is future work: an oversized directory returns Limit.

Some schema messages are reserved for future alternate workflows. Actual capability exchange is in Hello/HelloAck, published to callers only after authentication. File progress currently comes from transferred bytes as local Core events. Pause/resume is stream backpressure/offset recovery, and cancel resets the QUIC stream; standalone FILE_PROGRESS/FILE_CANCEL are not the active transfer control mechanism. Arbitrary reserved messages return Unsupported.

## Pairing and trust

1. The host issues a random 256-bit one-time ticket, expiring after 120 seconds.
2. `tyu://pair/<hex-postcard>` encodes version, socket endpoint (including port), ticket, expiry and host SHA-256 certificate fingerprint.
3. The initiating peer pins that fingerprint during TLS. The host requires a client certificate and verifies its TLS CertificateVerify signature, but keeps it quarantined until application admission.
4. Hello identities must match the IDs derived from TLS certificate fingerprints. Device-supplied names cannot substitute an identity.
5. PAIR_REQUEST atomically consumes the ticket before asking the host UI. A random client nonce travels in the protected channel; one-time-ticket state and TLS provide replay protection.
6. Explicit local approval persists the client's fingerprint and endpoint. PAIR_COMPLETE lets the initiator persist the host pin. PAIR_CONFIRM completes the exchange.
7. A later connection sends AUTH and must match stored, unrevoked trust. Revoked peers cannot re-pair silently; a future explicit recovery workflow is needed.

For discovery pairing, selecting an untrusted discovered peer sends PAIR_NEARBY_REQUEST instead of AUTH. TLS checks that the peer's certificate derives the selected discovery DeviceId and proves possession of its private key. The server must explicitly enable `allow_nearby_pairing` (Desktop does; Core defaults to false), and every initial request waits up to 120 seconds for local approval. No ticket is needed. Rejection creates no trust. Approval stores the full SHA-256 fingerprint on both ends; reconnect uses that full pin and AUTH. Revocation and quarantine still apply. Older Desktop binaries reject message 33 and must be updated.

The two stores cannot commit atomically across a failed network: if the final exchange is interrupted, one side may have saved trust first. Reconnect succeeds only when both sides have trust; otherwise pair again with explicit approval. Trust is never inferred from mDNS or device names. Discovery initially identifies the chosen advertised peer; unlike QR, it does not provide an independently scanned fingerprint. See SECURITY.md for this trust-on-first-use boundary.

## Sessions, input and clipboard

One display/mode session per peer; multiple independent service sessions may coexist. Each uses a UUID. The local requester applies a start only after ACK; the receiver validates capability pairs and current state. Connection loss removes sessions and media state. Session race recovery is currently disconnect/reconnect; no distributed transaction/resynchronization protocol is claimed.

Normalized touch/mouse coordinates must be finite in [0,1]. Touch contacts are 0–31. Wheel values, buttons and text lengths are validated. Orientation mapping translates normalized points to a receiver viewport. Delivery produces input events, not OS injection. Clipboard v1 is bounded UTF-8 text (65536 bytes), delivered to callers without automatically modifying the OS clipboard.

## Files and media

FileOffer contains transfer UUID, portable filename, size and BLAKE3. The receiver replies with a validated `.part` offset. The sender streams remaining bytes; the receiver rehashes its prefix plus incoming data, checks exact size and BLAKE3, then publishes the destination atomically without overwriting an existing file. The current no-clobber commit uses a hard link plus removal of the temporary link; filesystems without hard links return an I/O failure. Partial files and metadata are scoped by peer+transfer ID. Cancelled/interrupted partials remain resumable; local inbox maintenance is required. Resume after restarting the sender requires submitting the same transfer UUID and source path.

MediaPacket carries session/stream/frame IDs, fragment index/count, timestamp, flags and keyframe bit. Packetization shares `Bytes` slices and leaves space for headers under the caller-selected MTU. Reassembly tolerates reordering and duplicates, drops old completed-frame sequence numbers, expires incomplete frames and requests keyframes. Limits: 4 MiB/frame, 8192 fragments/frame, eight incomplete frames/peer. Latency stats currently measure assembly duration, not synchronized end-to-end latency. Capture, codec implementations, jitter/adaptive bitrate control and render/audio output are future adapters.
