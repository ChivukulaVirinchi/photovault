# Smriti Link — Complete Implementation Plan

Status: planning document only. No feature code has been implemented by this document.

This is the implementation contract for the phone-connectivity work discussed for Smriti. It is intentionally detailed enough that an engineer should not need to invent product behavior, security rules, storage semantics, or release policy while implementing it. If code and this document disagree during the initial implementation, stop and resolve the disagreement explicitly; do not silently improvise a different product.

---

## 1. The product in one sentence

Smriti Link lets a phone securely find a Smriti library on the same local network, browse that library, and transfer new phone photos into its owner’s folder on the library drive, while the drive remains the only source of truth and no photo passes through a cloud service.

The product has three names:

- **Smriti Link** is the local, authenticated protocol and capability.
- **Smriti Node** is the process attached to the library drive that answers Link requests.
- **Smriti Pocket** is the native phone companion.

The mental model must remain simple:

> The drive is the library. The Node opens a secure local door. Pocket is an inbox and a window.

Pocket is not a second Smriti library, a cloud backup, a peer-to-peer database replica, or a remote-control version of the desktop app.

---

## 2. Decisions that are already settled

These are requirements, not topics to reopen during implementation.

### 2.1 Product boundaries

1. Version 1 works only when Pocket and Node can reach each other on the same LAN/Wi-Fi network.
2. Link is disabled by default for every library.
3. Smriti does not create an account, run a hosted relay, store a photo remotely, open router ports, use UPnP, or require internet access for Link.
4. A drive merely being attached to an internet-capable device is not enough. A Smriti Node process must be running on a computer or home server that has direct filesystem access to the drive.
5. A basic router USB-share cannot host Link by itself. It only works if a computer or server mounts that share and runs Node, and network-mounted libraries are not a supported writer configuration in version 1.
6. The desktop app embeds Node when Link is enabled. A separate headless Node binary supports always-on Windows, macOS, Linux x64, and Linux ARM64 hosts.
7. Android receives a native Smriti Pocket application distributed as a signed APK through GitHub Releases. There is no Play Store dependency.
8. The native iOS implementation is written in the same repository and compiled/tested in macOS CI. No PWA is presented as iOS parity. No public IPA is promised until there is a sustainable Apple signing/distribution route.
9. Remote access through a secure tunnel may be added later. It must be a transport adapter beneath the same protocol, identity, authorization, and UI—not a protocol rewrite.
10. No unrelated photo-management feature is part of this project.

### 2.2 Ownership and multi-phone behavior

1. One library may authorize many phones.
2. Every phone creates and uses its own private signing key. Keys are never shared between spouses, family members, devices, or libraries.
3. Every phone belongs to a user-chosen owner such as “Virin” or “Mum.” Smriti never guesses ownership from faces.
4. Owner and device are separate records. A replacement phone can belong to the same owner without pretending to be the old device.
5. Imported files are organized as:

       Phone Imports/<owner_slug>/<device_slug>/<YYYY>/<MM>/<safe_filename>

6. This directory separation is organizational, not cryptographic access control. Anyone who can browse the whole drive can see all files.
7. A phone is independently revocable. Revoking one phone does not disturb the others.
8. Initial capabilities are exactly:
   - upload only
   - browse and upload
9. Pocket cannot delete drive files, empty trash, merge people, edit albums, alter memories, change desktop settings, or administer other phones.
10. Different phones uploading byte-identical photos are preserved as separate owner/device imports. Existing duplicate tools may surface them later. Link does not silently collapse family members’ originals.

### 2.3 Source cleanup behavior

1. “Offer to free phone space after verified transfer” is off by default.
2. Smriti never silently deletes phone media in the background.
3. An item becomes eligible for cleanup only after every required resource has been durably written to the drive, verified byte-for-byte, committed to Smriti’s provenance ledger, and covered by a signed receipt.
4. The phone stores and verifies that receipt before showing the item in the cleanup review.
5. The operating system’s normal trash/delete confirmation must be used.
6. Cleanup affects only source media on that phone. It never removes the drive copy.
7. The UI says “verified transfer,” not “backup.” One removable drive is not automatically a backup.
8. The confirmation warns that deleting an item from Apple Photos, iCloud Photos, Google Photos, or another synchronized gallery may delete it on other devices too.

### 2.4 Security behavior

1. Pairing requires physical access to the desktop/headless Node display and a one-use QR code.
2. The QR token expires after five minutes and exists only in Node memory.
3. Both sides show the same short authentication code; the user confirms it on the host before authorization is persisted.
4. The phone proves its identity with a hardware/OS-protected device key. The Node proves the portable library identity whose public-key fingerprint was pinned from the QR code.
5. All network traffic, including thumbnails, is encrypted and authenticated.
6. Discovery is never authorization.
7. No network caller receives a raw filesystem path, a SQLite connection, or unrestricted Tauri command access.
8. There is no anonymous mode.
9. There is one active writer process per physical library. Desktop-embedded Node shares that process and database connection. A second desktop/Node process must fail safely.
10. A library unplug, I/O failure, verification mismatch, database failure, or receipt failure means no cleanup eligibility.

---

## 3. Explicit non-goals for version 1

Keeping these out is how the feature stays recognizably Smriti rather than becoming a cloud platform.

- Internet-wide access, NAT traversal, relay servers, or WireGuard management.
- User accounts, passwords, email recovery, subscriptions, telemetry, or hosted storage.
- Peer-to-peer database replication.
- Keeping a complete second photo library on the phone.
- Mobile face management, duplicate management, album editing, trash management, or settings parity.
- Automatic deletion from the phone.
- Inferring which family member owns a phone from photo content or face clusters.
- Cross-device duplicate removal during transfer.
- Re-encoding, recompressing, resizing, or “optimizing” uploaded originals.
- Importing cloud-only placeholders without an explicit future product decision.
- Silently exposing a Node through a public interface or changing firewall/router settings broadly.
- A web/PWA fallback described as equivalent to the native iOS client.
- Public iOS distribution without valid Apple signing and a documented maintenance path.
- Supporting simultaneous writers against the same SQLite library over SMB/NFS.

When a requested implementation step appears to require one of these, record it as a future proposal and continue without it.

---

## 4. What already exists and must be reused

Do not build parallel substitutes for working Smriti services.

| Existing foundation | How Link uses it |
|---|---|
| src/db and schema migrations | Add Link metadata and provenance using the same SQLite database and migration discipline. |
| PhotoRepo and scanner | Register transferred visual resources, then run the normal metadata/indexing path. |
| ThumbnailService | Serve the same generated thumbnails used by desktop grids. |
| Semantic search and existing search services | Execute search on Node and return only results to Pocket. Embeddings never need to leave the library. |
| Album, people, memory, and timeline repositories/services | Back read-only Pocket browsing endpoints. |
| Google Takeout streaming importer | Supply reusable ideas and code for staging, free-space checks, hashing, safe names, atomic placement, idempotency, and recovery. |
| src-tauri command conventions | Guide stable DTOs and thin adapters, but Link must not expose Tauri IPC over the network. |
| AppState/OpenLibrary | Supply the live database and services to an embedded Node without opening the library twice. |
| Existing updater/release checksums | Guide direct APK and Node binary release/update verification. |
| scripts/ci_local.sh | Remain the mandatory desktop pre-push gate. Mobile and Node checks are added alongside it. |

Important distinction: the scanner’s current fast file hash is useful for indexing, but it is not a durable-transfer proof. Link must calculate a full SHA-256 digest over every transferred resource and store it in the Link provenance/receipt records.

---

## 5. Target architecture

### 5.1 Data flow

    Phone photo library
          |
          | Native platform adapter enumerates local assets
          | and stages one resource at a time in app-private storage
          v
    Smriti Pocket Rust sync engine
          |
          | Pinned TLS + authenticated /link/v1 protocol
          | resumable raw-byte chunks
          v
    Smriti Node upload service
          |
          | .photovault/link/incoming staging on the same drive
          | SHA-256 verification + fsync + atomic rename
          v
    Phone Imports/<owner>/<device>/<year>/<month>
          |
          | one durable SQLite transaction + signed receipt
          v
    Existing Smriti scanner, metadata, thumbnail, face,
    semantic, memory, and suggestion pipelines

For browsing, the arrow reverses only at the presentation boundary:

    Existing Smriti DB/search/services
          -> small paginated DTOs and existing thumbnails
          -> encrypted LAN connection
          -> bounded Pocket cache

No SQLite file, model file, raw internal path, or database journal is sent to Pocket.

### 5.2 Process arrangements

Desktop arrangement:

    smriti-tauri process
      - owns LibraryLease
      - owns Database/OpenLibrary
      - owns embedded Smriti Node
      - owns desktop Tauri commands

Headless arrangement:

    smriti-node process
      - owns LibraryLease
      - owns Database and required services
      - owns LAN server and local admin socket

Never start a second Node process from the desktop. Embedded Node is a service inside the already-open desktop process.

### 5.3 Repository layout

Add the following without moving unrelated files:

    crates/
      smriti-link-protocol/
        Cargo.toml
        src/
          lib.rs
          version.rs
          ids.rs
          dto.rs
          errors.rs
          canonical.rs
          crypto.rs
          receipt.rs
      smriti-node/
        Cargo.toml
        src/
          lib.rs
          config.rs
          identity.rs
          discovery.rs
          tls.rs
          auth.rs
          pairing.rs
          sessions.rs
          router.rs
          browse.rs
          upload.rs
          recovery.rs
          limits.rs
          admin.rs
    src-node/
      Cargo.toml
      src/
        main.rs
        cli.rs
        runtime.rs
        admin_client.rs
    src-pocket/
      Cargo.toml
      tauri.conf.json
      capabilities/
      src/
        lib.rs
        state.rs
        commands.rs
        client.rs
        sync/
          mod.rs
          planner.rs
          worker.rs
          ledger.rs
          cleanup.rs
        cache.rs
        discovery.rs
        platform.rs
      gen/
        android/
        apple/
    plugins/
      smriti-mobile-media/
        Cargo.toml
        src/
        android/
        ios/
    src-ui/src/pocket/
      PocketApp.svelte
      routes/
      components/
      stores/
      api/

Keep one Svelte package in src-ui. Add separate desktop and Pocket entry points/build outputs rather than a second copy of package.json and node_modules:

- Desktop continues to build into src-ui/dist.
- Pocket builds into src-ui/dist-pocket.
- Vite selects the entry through an explicit build mode, not browser user-agent detection.
- Shared colors, typography, logo, buttons, error components, and photo tiles stay in src-ui/src/lib.
- Desktop-only Tauri calls stay under the existing desktop API module.
- Pocket calls only src-pocket commands through src-ui/src/pocket/api.

Add all Rust crates to the root Cargo workspace. Keep the pure protocol crate free of Tauri, filesystem, database, image, ONNX, and platform dependencies.

### 5.4 Dependency rules

1. smriti-link-protocol may depend only on serialization, identifiers, hashing/signature types, and small error helpers.
2. smriti-node depends on smriti and smriti-link-protocol.
3. smriti-tauri may depend on smriti-node to embed it.
4. src-node depends on smriti-node.
5. src-pocket depends on smriti-link-protocol and the mobile media plugin. It must not depend on smriti’s desktop database or ML engine.
6. Platform Kotlin/Swift code implements media-library, secure-key, local-network, and cleanup adapters only. Upload protocol/state logic remains Rust.
7. The Svelte UI never handles original media bytes or private keys.
8. Future remote transport may implement the client dial/discovery trait. It may not bypass session authentication or change application endpoints.

Do not feature-gate or split the existing ML engine merely to make an early build smaller. First make the correct shared service work. If headless ARM64 packaging later demonstrates a concrete ONNX problem, introduce a separately reviewed engine feature boundary with semantic-search behavior and tests explicitly preserved.

---

## 6. Portable library identity and single-writer lease

Implement this before opening a socket.

### 6.1 On-drive layout

When Link is enabled, create:

    .photovault/
      library.lock
      link/
        identity/
          library.json
          identity-key.pk8
          identity-certificate.der
          tls-key.pk8
          tls-certificate.der
        incoming/
        receipts/
        recovery/

Rules:

- Paths are relative to the selected library root.
- No secret is written outside the library merely to make portability work.
- incoming and all final destinations are on the same filesystem so final rename is atomic.
- identity-key.pk8 is never logged, included in diagnostics, returned over an endpoint, or copied into a QR.
- On filesystems supporting permissions, restrict identity private-key files to the current user. FAT/exFAT may not enforce those permissions; document the physical-drive threat boundary.
- library.json contains a random library UUID, identity format version, public-key fingerprint, and creation time. It contains no library display name, owner name, or photo count.

### 6.2 Identity algorithm

Use standard ECDSA P-256 with SHA-256 for the portable library signing identity.

- Generate the key with an audited cryptographic random-number generator.
- Store the private key as PKCS#8.
- Store/export the public key as DER SubjectPublicKeyInfo.
- The public-key pin is SHA-256 over the DER SubjectPublicKeyInfo, base64url encoded.
- Use the identity key to sign receipts and to sign a separate rotatable P-256 TLS leaf certificate. Do not reuse the leaf private key for receipts.
- Make the identity certificate a pinned local trust root and include the opaque library UUID in the signed TLS leaf identity.
- Pin the identity public key, not the entire leaf certificate, so the TLS key/certificate can be renewed without re-pairing.
- Never implement elliptic-curve math or signature encoding manually.

On first enable:

1. Ensure .photovault/link/identity exists.
2. If no identity exists and there are no Link database records, generate it.
3. Write private key, public metadata, and certificate through .partial files.
4. sync_all each file.
5. atomically rename each file.
6. sync the parent directory where supported.
7. read everything back and verify that the private key signs a test message accepted by the stored public key.
8. Only then mark Link enabled.

If identity files are missing/corrupt while paired devices exist, do not silently generate a replacement. Show “Link identity is damaged; reset Link to revoke all phones and create a new identity.” Reset requires explicit confirmation.

### 6.3 Single-writer lease

Add an engine-level LibraryLease using the already-present fs2 dependency.

Required behavior:

1. Resolve and validate the library root.
2. Create/open .photovault/library.lock without truncating it.
3. Call try_lock_exclusive and keep the File handle alive for the whole library session.
4. After acquiring the lock, write diagnostic JSON containing process ID, host label, application kind, and start time. This metadata is informational; the OS lock is authoritative.
5. If locking fails, return a typed LibraryAlreadyOpen error with safe diagnostic text.
6. Release through the fs2 FileExt unlock method during orderly shutdown. A process crash releases the OS lock automatically.
7. Never delete the lock file as an unlocking mechanism.

Acquire the lease in every supported top-level entry:

- desktop library open
- standalone Node serve
- any maintenance command that writes the library without going through one of those sessions

The embedded Node receives the desktop’s already-open handles and never requests a second lease or SQLite connection.

Tests:

- first lease succeeds
- concurrent lease fails
- dropping the first lease permits another
- stale text in the lock file does not block acquisition
- two different libraries may be opened
- paths resolving to the same library cannot bypass the lease with relative segments
- desktop Link start does not reacquire the lease

Network filesystems and cloned drives cannot provide all local-filesystem guarantees. Version 1 documentation must say the supported Node configuration is a drive locally attached/mounted to the one writer host. If a user clones a library, the copied drive also clones its Link identity; provide a documented “Reset Link identity on this copy” recovery action rather than pretending two offline clones can coordinate.

---

## 7. Database migration and data ownership

The current schema version is 28. Implement Link in one forward-only migration to version 29 unless another feature lands first; in that case use the next unused version and update every reference consistently.

### 7.1 Tables

Add these tables to both the fresh schema and migration.

#### link_owners

| Column | Type/rules | Meaning |
|---|---|---|
| id | TEXT PRIMARY KEY | Random UUID. |
| display_name | TEXT NOT NULL | User-facing name. |
| storage_slug | TEXT NOT NULL UNIQUE | Immutable safe directory component. |
| created_at | INTEGER NOT NULL | Unix milliseconds. |
| updated_at | INTEGER NOT NULL | Unix milliseconds. |

Changing display_name must not rename existing folders. storage_slug is chosen once, collision-safe, and immutable.

#### link_devices

| Column | Type/rules | Meaning |
|---|---|---|
| id | TEXT PRIMARY KEY | Random UUID assigned during pairing. |
| owner_id | TEXT NOT NULL FK link_owners | Human-selected owner. |
| display_name | TEXT NOT NULL | For example “Virin’s Pixel.” |
| storage_slug | TEXT NOT NULL | Immutable per-owner directory component. |
| platform | TEXT CHECK android/ios | Client platform. |
| key_algorithm | TEXT CHECK p256-sha256 | Versioned algorithm marker. |
| public_key_der | BLOB NOT NULL UNIQUE | Device verification key. |
| capability | TEXT CHECK upload_only/browse_upload | Authorization boundary. |
| status | TEXT CHECK active/revoked | Revocation state. |
| protocol_version | INTEGER NOT NULL | Last negotiated protocol. |
| app_version | TEXT | Last reported Pocket version. |
| paired_at | INTEGER NOT NULL | Unix milliseconds. |
| last_seen_at | INTEGER | Updated at a bounded frequency, not every request. |
| revoked_at | INTEGER | Set once revoked. |

Unique constraint: owner_id plus storage_slug. A revoked device ID/key is never silently reactivated; pairing creates a new record.

#### link_uploads

| Column | Type/rules | Meaning |
|---|---|---|
| id | TEXT PRIMARY KEY | Server-generated upload UUID. |
| device_id | TEXT NOT NULL FK link_devices | Sending phone. |
| asset_key | TEXT NOT NULL | Opaque phone-local asset identity. |
| manifest_hash | TEXT NOT NULL | SHA-256 of canonical manifest. |
| state | TEXT NOT NULL CHECK | receiving, verifying, placed, committed, receipt_issued, failed, cancelled. |
| captured_at | INTEGER | Platform capture time fallback. |
| timezone_offset_minutes | INTEGER | Capture-time offset when known. |
| total_bytes | INTEGER NOT NULL | Sum of all declared resources. |
| received_bytes | INTEGER NOT NULL DEFAULT 0 | Recoverable progress. |
| destination_date_source | TEXT | capture, file, or import. |
| error_code | TEXT | Stable safe failure code. |
| created_at | INTEGER NOT NULL | Unix milliseconds. |
| updated_at | INTEGER NOT NULL | Unix milliseconds. |

Unique constraint: device_id, asset_key, manifest_hash. This is the main idempotency key.

#### link_upload_resources

| Column | Type/rules | Meaning |
|---|---|---|
| upload_id | TEXT FK link_uploads | Parent upload. |
| resource_id | TEXT | Phone-generated stable resource UUID. |
| role | TEXT NOT NULL | primary_photo, paired_video, raw, rendered_edit, original, adjustment, or sidecar. |
| original_name | TEXT NOT NULL | Display/provenance only; never trusted as a path. |
| media_type | TEXT | Claimed MIME type. |
| expected_size | INTEGER NOT NULL | Exact bytes. |
| expected_sha256 | TEXT NOT NULL | Lowercase 64-character digest. |
| received_bytes | INTEGER NOT NULL DEFAULT 0 | Next accepted sequential offset. |
| staged_relative_path | TEXT NOT NULL | Contained beneath link/incoming. |
| final_relative_path | TEXT | Set after placement. |
| final_object_id | TEXT | Opaque UUID assigned to the durable destination. |
| photo_id | INTEGER FK photos | Set only for resources indexed as media. |
| actual_sha256 | TEXT | Set after full server verification. |
| state | TEXT NOT NULL | pending, receiving, verified, placed, committed, failed. |

Primary key: upload_id plus resource_id.

#### photo_sources

| Column | Type/rules | Meaning |
|---|---|---|
| id | INTEGER PRIMARY KEY | Local row ID. |
| photo_id | INTEGER FK photos | Indexed Smriti item. |
| owner_id | TEXT NOT NULL FK link_owners | Selected owner. |
| device_id | TEXT NOT NULL FK link_devices | Source phone. |
| upload_id | TEXT NOT NULL FK link_uploads | Transfer. |
| asset_key | TEXT NOT NULL | Opaque source asset. |
| resource_id | TEXT NOT NULL | Source resource. |
| resource_role | TEXT NOT NULL | Role from manifest. |
| original_name | TEXT NOT NULL | Original filename. |
| full_sha256 | TEXT NOT NULL | Transfer integrity digest. |
| imported_at | INTEGER NOT NULL | Commit time. |

Unique constraint: upload_id plus resource_id. The parent upload’s unique device_id/asset_key/manifest_hash tuple supplies the remaining idempotency boundary. Add an index on photo_id and another on owner_id/imported_at.

#### link_receipts

| Column | Type/rules | Meaning |
|---|---|---|
| id | TEXT PRIMARY KEY | Receipt UUID. |
| upload_id | TEXT NOT NULL UNIQUE FK link_uploads | Exactly one receipt per completed upload. |
| canonical_payload | BLOB NOT NULL | Exact signed RFC 8785 JSON bytes. |
| signature_der | BLOB NOT NULL | ECDSA signature. |
| issued_at | INTEGER NOT NULL | Unix milliseconds. |
| phone_acknowledged_at | INTEGER | Optional sync bookkeeping. |

### 7.2 Indices

Create at least:

- link_devices(status)
- link_devices(owner_id)
- link_uploads(device_id, state, updated_at)
- link_upload_resources(upload_id, state)
- photo_sources(photo_id)
- photo_sources(owner_id, imported_at)
- link_receipts(upload_id)

### 7.3 Migration rules

1. Create all tables and indices inside one migration transaction.
2. Existing libraries gain no identity and open no port merely by migrating.
3. Link remains disabled until the user enables it.
4. Foreign-key actions must preserve photo rows independently of revoked-device status. Revocation changes status; it does not delete provenance.
5. Never cascade owner/device deletion into photo deletion.
6. Owners with imports cannot be deleted; they may be renamed.
7. A failed migration leaves schema version 28 and no partial Link tables.
8. Add migration tests for a representative version-28 fixture and for a fresh database.
9. Preserve the existing “database newer than app” rejection behavior.
10. Add repository methods; do not scatter raw Link SQL through HTTP handlers.

### 7.4 Phone-local ledger

Pocket needs its own small SQLite database in app-private storage. It is a transfer journal, not a photo catalog.

Tables:

- paired_libraries: library ID, pinned public-key hash, friendly name, last endpoints, protocol range, paired time.
- local_device: device ID per library, public-key reference, capability, owner display name.
- local_assets: opaque asset key, platform-local lookup token, current manifest hash, discovery time, cleanup status.
- transfer_resources: asset key, resource ID, staged path, size, SHA-256, next server offset, state.
- receipts: receipt ID, library ID, asset key, canonical payload, signature, verified time.
- pocket_settings: Wi-Fi-only, charging preference, cleanup offer, cache limit.

The platform’s raw Photos/MediaStore identifier stays only on that phone. Before sending, Pocket maps it to a random UUID stored in local_assets. Reinstalling Pocket may lose that mapping; a content-hash match can warn about a likely prior import, but must not silently delete or merge anything.

---

## 8. Shared protocol contract

The protocol lives under /link/v1 and is versioned independently of desktop UI commands.

### 8.1 Wire choices

- HTTPS over TCP using rustls.
- JSON for small control DTOs.
- Raw request/response bodies for thumbnails, originals, and upload chunks.
- RFC 8785 canonical JSON for anything hashed or signed.
- SHA-256 for content hashes.
- ECDSA P-256/SHA-256 for identity signatures.
- UUIDs serialized in lowercase hyphenated form.
- Unix timestamps serialized as milliseconds.
- Relative media paths never cross the public API.
- Unknown JSON fields are tolerated on reads; required fields remain strict.
- Every response carries X-Smriti-Protocol and X-Request-Id.

Select maintained crates compatible with the repository toolchain at implementation time; record the exact versions in Cargo.lock. Do not substitute handwritten crypto, HTTP parsing, certificate parsing, canonicalization, or mDNS packets.

### 8.2 Version negotiation

GET /link/v1/hello returns:

    {
      "library_id": "...",
      "protocol_min": 1,
      "protocol_max": 1,
      "node_version": "...",
      "public_key_pin": "...",
      "features": ["browse", "upload", "resumable_upload", "signed_receipts"]
    }

The client chooses the highest overlapping protocol. If there is no overlap, show a specific “Update Smriti Pocket” or “Update Smriti on the host” message. Never fall back to an unversioned route.

### 8.3 Error envelope

Every non-byte error uses:

    {
      "error": {
        "code": "stable_machine_code",
        "message": "safe user-facing summary",
        "retryable": false,
        "request_id": "..."
      }
    }

Minimum codes:

- protocol_incompatible
- pairing_closed
- pairing_expired
- pairing_already_used
- confirmation_required
- invalid_signature
- device_revoked
- capability_denied
- session_expired
- rate_limited
- invalid_manifest
- unsupported_media
- invalid_offset
- hash_mismatch
- upload_not_found
- upload_failed
- library_unavailable
- insufficient_space
- drive_removed
- indexing_pending
- feature_unavailable
- internal_error

No error includes a private key, token, SQL statement, absolute path, stack trace, or another device’s details.

### 8.4 Session establishment

Normal paired connection:

1. Pocket discovers a Node advertising the paired opaque library ID.
2. Pocket opens TLS, verifies the leaf certificate chains to the pinned library identity public key, and verifies the signed leaf contains the expected library UUID.
3. Pocket requests a one-use 32-byte challenge. It expires in 60 seconds.
4. Pocket constructs a canonical transcript containing protocol version, library ID, device ID, challenge, client nonce, and current timestamp.
5. The native secure-key adapter signs SHA-256 of that transcript.
6. Pocket sends transcript and signature.
7. Node loads the active device record, checks exact library/device IDs, freshness, one-use challenge, algorithm, signature, and capability.
8. Node returns a random 256-bit bearer session token held only in memory.
9. Session lifetime is 15 minutes with a bounded idle timeout. Reauthentication is transparent.
10. Session tokens are bound to library, device, capability, and protocol version and compared in constant time.

Do not sign every upload chunk; pinned TLS plus a short-lived authenticated session provides integrity and avoids unnecessary platform-key prompts.

### 8.5 Endpoint list

Unauthenticated but TLS-pinned:

- GET /link/v1/hello
- POST /link/v1/pair
- POST /link/v1/session/challenge
- POST /link/v1/session/open

Authenticated:

- GET /link/v1/status
- POST /link/v1/uploads
- GET /link/v1/uploads/{upload_id}
- PUT /link/v1/uploads/{upload_id}/resources/{resource_id}
- POST /link/v1/uploads/{upload_id}/complete
- GET /link/v1/receipts/{receipt_id}
- POST /link/v1/receipts/{receipt_id}/acknowledge
- GET /link/v1/timeline
- POST /link/v1/search
- GET /link/v1/photos/{photo_id}
- GET /link/v1/photos/{photo_id}/thumbnail
- GET /link/v1/photos/{photo_id}/original
- GET /link/v1/albums
- GET /link/v1/albums/{album_id}
- GET /link/v1/people
- GET /link/v1/people/{person_id}
- GET /link/v1/memories
- GET /link/v1/memories/{memory_id}

Administrative pairing/revocation is not exposed through these authenticated phone routes.

### 8.6 Pagination

Reuse Smriti’s cursor conventions:

- Default page: 60 grid items.
- Maximum page: 200.
- Cursor is opaque and signed or server-validated; it does not expose SQL.
- Stable ordering uses capture timestamp plus photo ID as a tie-breaker.
- A malformed/expired cursor receives invalid_cursor, not a server error.
- Response includes items, next_cursor, and has_more.

### 8.7 Byte endpoints

- Thumbnail response includes Content-Type, Content-Length, ETag, Cache-Control: private, and supports If-None-Match.
- Original/video response supports standard single byte Range requests and 206 responses.
- Do not load an original fully into memory.
- Do not base64 media into JSON or Tauri IPC.
- ETags derive from immutable content hash plus rendition parameters.
- A revoked/expired session cannot reuse a previously authorized URL because URLs contain no bearer token.

---

## 9. Pairing and discovery

### 9.1 Discovery

Use mDNS/Bonjour service type:

    _smriti._tcp.local

Advertise only:

- opaque library UUID
- protocol minimum/maximum
- chosen port
- short public-key-pin prefix for collision diagnostics

Do not advertise the library name, owner names, device names, file paths, photo count, or whether a particular person is home.

Node attempts the configured preferred port first. If it is occupied, bind an OS-assigned port and advertise the actual port. Pocket also remembers the last successful endpoints but treats mDNS as the normal source.

If guest Wi-Fi/client isolation blocks peers or multicast, Pocket shows:

> Smriti can see this Wi-Fi but devices on it may be isolated. Use a normal home network, or show a reconnect QR on the Smriti host.

A reconnect QR contains signed locator information and the existing library public-key pin. It does not authorize a new device or rotate identity.

### 9.2 Pairing QR payload

Use the custom scheme smriti://pair with:

- format version
- library UUID
- public-key pin
- one or more current LAN endpoint hints
- random 256-bit one-use token
- expiry timestamp

Do not include:

- private key material
- a reusable session token
- database paths
- owner/photo metadata
- a token valid beyond five minutes

The QR may be rendered by the desktop modal, printed as a terminal QR by the headless CLI, or saved as a short-lived local PNG only when explicitly requested. Any saved QR is deleted/invalidated when pairing completes or expires.

### 9.3 Desktop pairing flow

1. User opens Settings → Phone Sync.
2. If Link is disabled, explain the local-network behavior and ask the user to enable it.
3. User selects an existing owner or enters a new owner name.
4. User gives the phone a display name.
5. User chooses upload only or browse and upload.
6. Node creates an in-memory PairingAttempt with token and five-minute expiry.
7. Modal shows QR, expiry countdown, and Cancel.
8. Pocket scans, pins the library public key, creates/loads its device key, and sends platform, display name, public key, and token.
9. Node computes a six-digit short authentication string from the canonical pairing transcript and shows it on desktop.
10. Pocket shows the same six digits and asks the user to compare them.
11. User presses “Codes match” on the host.
12. Only now does Node transactionally create owner/device records.
13. Pairing token is destroyed and cannot be reused.
14. Pocket receives device ID/capability, stores pairing data, performs a normal signed session handshake, and shows success.

If the host declines or the codes differ, store no device. Rate-limit failed attempts and close that token.

### 9.4 Headless pairing flow

The running smriti-node exposes an admin interface only through an OS-local IPC endpoint:

- Unix domain socket under the current user’s runtime directory on Linux/macOS.
- Named pipe scoped to the current user on Windows.

It must never bind the admin API to a LAN address.

Commands:

    smriti-node status --library <path>
    smriti-node owners --library <path>
    smriti-node pair --library <path> --owner "Virin" --device "Phone" --capability browse-and-upload
    smriti-node devices --library <path>
    smriti-node revoke --library <path> --device <id>

pair asks the running process to open PairingAttempt, renders the QR in the terminal, displays the short code, and prompts for confirmation. If the service is not running, it says exactly how to start it. The CLI does not open the SQLite database while the server owns the lease.

### 9.5 Device secure key

Use a native non-exportable P-256 signing key:

- Android: Android Keystore, with an application-specific alias containing the library ID.
- iOS: Keychain/SecKey, using Secure Enclave when available and an OS-protected Keychain key otherwise.

The mobile plugin exposes only:

- create_or_load_key(library_id)
- public_key_spki(library_id)
- sign_digest(library_id, sha256_digest)
- delete_key(library_id) after explicit unpair

Rust never receives the private key bytes. Do not require biometric prompts for routine background signing; use after-first-unlock/device-protected access appropriate for background work.

### 9.6 Revocation

Desktop/headless admin:

1. Show device name, owner, capability, paired date, last seen, and last successful transfer.
2. Ask for confirmation.
3. Set status revoked and revoked_at in one transaction.
4. Remove all live sessions/challenges for that device.
5. Keep transfer receipts/provenance.
6. A currently uploading request fails before its next commit and never receives a receipt.

Pocket receiving device_revoked deletes its sessions, stops sync, retains local source media, and offers to remove the stale pairing/key.

---

## 10. Safe ingest pipeline

This is the highest-risk data path. Implement it as a reusable engine service, not as HTTP-handler code.

### 10.1 Extract shared primitives

Refactor, without changing Google Takeout behavior, the safe pieces currently embedded in the Takeout importer into an engine module such as src/services/safe_ingest.rs:

- validate relative destination containment
- sanitize path components and filenames
- choose collision-safe final names
- calculate required/free space
- create a same-filesystem staging file
- stream bytes while calculating full SHA-256
- sync a file
- atomically place it
- sync parent directory where supported
- recover .partial files

Keep Takeout-specific ZIP/sidecar logic in the Takeout importer. Add regression tests proving Takeout imports still resume, avoid collisions, and preserve bytes.

### 10.2 Manifest

Pocket creates one manifest per platform asset:

    {
      "asset_key": "opaque UUID",
      "captured_at": 1780000000000,
      "timezone_offset_minutes": 330,
      "width": 4032,
      "height": 3024,
      "resources": [
        {
          "resource_id": "UUID",
          "role": "primary_photo",
          "original_name": "IMG_1234.HEIC",
          "media_type": "image/heic",
          "byte_length": 8450123,
          "sha256": "..."
        }
      ]
    }

Rules:

- Resources are sorted by resource_id before canonicalization.
- resource_id is unique inside the asset.
- byte_length must be non-negative and within configured per-resource limit.
- names are labels only; Node derives paths.
- location need not be transmitted separately because the original bytes/EXIF remain authoritative.
- captured_at is a fallback for resources whose exported bytes do not retain usable metadata.
- A changed/edited platform asset produces a new manifest hash and is imported as a new preserved rendition; never overwrite the prior import.

Required resource groups include all resources the platform reports as belonging to the asset:

- Live Photo still plus paired video
- RAW plus JPEG when both belong to the same asset
- original/rendered resources exposed by the platform
- meaningful adjustment/sidecar resources

Sidecars are preserved adjacent to their visual resource and recorded in provenance even if the normal scanner does not index them.

### 10.3 Begin or resume

POST /uploads:

1. Authenticate upload capability.
2. Validate manifest structure, count, sizes, extensions, media classifications, and canonical hash.
3. Look up device_id plus asset_key plus manifest_hash.
4. If a receipt already exists, return the same signed receipt. Do not create another file.
5. If an incomplete upload exists, compare the manifest exactly and return every resource’s next_offset.
6. Otherwise check free space and in-flight quotas.
7. Create upload/resource rows and contained staging paths in one transaction.
8. Create staging directories beneath:

       .photovault/link/incoming/<device_id>/<upload_id>/

9. Return upload ID, chunk limit, and next offsets.

Never trust a client-provided upload ID as a filesystem component without UUID parsing.

### 10.4 Chunk upload

Use sequential chunks, initially capped at 4 MiB.

PUT resource request includes:

- offset query/header
- exact Content-Length
- X-Chunk-SHA256
- raw bytes

Server behavior:

1. Ensure device owns the upload and is still active.
2. Reject a body larger than the negotiated maximum before reading it.
3. Lock only that upload/resource, not the whole server.
4. Compare offset to actual staging-file length and database received_bytes.
5. If the file is shorter after a crash, rewind ledger to actual length.
6. If the file is longer than ledger, truncate to the ledger offset unless a verified recovery record proves otherwise.
7. Stream the body through SHA-256; never buffer the whole chunk.
8. Reject and leave offset unchanged if chunk hash or length differs.
9. Append at exactly the accepted offset.
10. Flush userspace buffers.
11. Transactionally update resource/upload received bytes.
12. Return next_offset.

Do not sync_all after every 4 MiB chunk; that would punish HDD performance. Crash recovery reconciles the actual file. Full durability is required at completion.

Initial concurrency limits:

- at most two resource streams per device
- at most four resource streams per Node
- exactly one finalization writer per spinning library drive
- configurable downward, with conservative defaults

Return retryable rate_limited with Retry-After rather than spawning unbounded work.

### 10.5 Verification and placement

POST /uploads/{id}/complete performs:

1. Verify every resource has exactly expected_size bytes.
2. Re-read every complete staged resource sequentially and calculate full SHA-256.
3. Compare in constant time with expected SHA-256.
4. Validate magic/container type against the declared role without decoding the whole media in the network request.
5. Reject executable/path-like/unsupported content. Keep failure diagnostic but never make it cleanup-eligible.
6. Select destination year/month from valid capture timestamp; fall back to file metadata, then import date. Record which source was used.
7. Resolve immutable owner/device slugs from database, never from the request.
8. Sanitize the original base name. Remove separators, reserved Windows device names, control characters, trailing dots/spaces, and excessive length.
9. Choose a deterministic collision-safe name. Idempotency lookup happens before suffix generation.
10. Ensure the resolved destination remains beneath the library root.
11. sync_all the staged file.
12. Atomically rename it to the final destination.
13. sync the final file and its parent directory where the platform supports it.
14. Mark resource placed.

If one asset has several required resources, do not issue a receipt until all are placed. If placement partially succeeds and the process crashes, recovery resumes from recorded paths; it never creates a second suffixed copy blindly.

### 10.6 Database commit and receipt

For each indexable visual resource:

1. Create/reuse the normal photo row using the final relative path and full SHA-256.
2. Store captured_at fallback only when normal metadata is absent.
3. Add photo_sources provenance.
4. Mark resource committed.

For non-indexed sidecars, add provenance tied to the upload and final path through the Link resource row.

Commit upload state with a durability-sensitive SQLite transaction. The implementation must temporarily use FULL synchronous semantics for the final Link commit or provide an equivalently tested durable connection; restore the project’s normal setting after the transaction.

Then create a canonical receipt containing:

- receipt format version
- protocol version
- receipt UUID
- library UUID
- device UUID
- upload UUID
- opaque asset key
- manifest hash
- every resource ID, role, byte length, full SHA-256, and opaque final_object_id
- committed timestamp

The receipt never exposes the destination path. Node retains final_relative_path in its private database for recovery; Pocket receives only the opaque final_object_id.

Receipt sequence:

1. Canonicalize payload.
2. Sign its SHA-256 digest with library identity.
3. Verify the signature locally.
4. Write payload/signature envelope to .photovault/link/receipts/<receipt_id>.partial.
5. sync_all.
6. Rename to .json and sync parent directory.
7. Insert link_receipts and mark upload receipt_issued in a durable transaction.
8. Read the receipt file back and verify it.
9. Return it to Pocket.

If the photo metadata/thumbnail/face/semantic follow-up job fails, the durable receipt remains valid because the original bytes are safe. Show “Transferred; still indexing” rather than forcing the phone to re-upload.

### 10.7 Free-space protection

Before accepting a new upload, require:

    available space >= remaining upload bytes + safety reserve

Safety reserve:

    max(1 GiB, min(5% of drive capacity, 10 GiB))

Also cap:

- resources per asset
- bytes per resource
- total in-flight bytes per device
- total in-flight bytes per Node
- stale incomplete-upload lifetime

Make limits configurable internally with secure defaults and expose only useful failure text. A compromised paired phone must not be able to fill the entire drive or allocate millions of rows.

### 10.8 Recovery

Run recovery before advertising Node availability:

| Observed state | Recovery action |
|---|---|
| receiving with partial file | Reconcile file length and keep resumable. |
| verifying with complete file | Recompute digest and continue or mark hash_mismatch. |
| placed, database not committed | Validate final file/hash, then commit provenance. |
| committed, receipt absent | Regenerate the deterministic receipt and persist it. |
| receipt file exists, DB row absent | Verify signature/file hashes and reconstruct the receipt row. |
| receipt issued | Return the same receipt on retry. |
| orphan staging directory with no row | Quarantine, then purge only after retention period and diagnostic logging. |
| final file missing after claimed placement | Mark failed; never issue receipt. |

Recovery must be idempotent. Running it twice produces the same rows and paths.

### 10.9 Drive removal

On removable-drive I/O errors or existing drive-disconnect events:

1. Stop accepting new uploads.
2. Cancel/finish in-memory reads without claiming completion.
3. Do not issue a receipt for an unverified operation.
4. Stop advertising availability if the library is no longer accessible.
5. Preserve Pocket queue state.
6. When the same library identity returns on any authorized Node host, Pocket rediscovers it and resumes from server-reported offsets.

No “success” toast may be based solely on bytes sent by the phone.

---

## 11. Pocket sync engine

### 11.1 Native adapter contract

Generate the Tauri mobile plugin using the supported plugin scaffolding, then implement the same Rust-facing interface on Android and iOS.

Media adapter operations:

- request_library_permission
- permission_status
- enumerate_local_assets(after_cursor, limit)
- describe_asset(platform_token)
- stage_resource(platform_token, resource_role, destination)
- release_staged_resource(path)
- request_system_trash(platform_tokens)

Secure-key operations are listed in section 9.5.

Network/background operations:

- current_network_is_local_wifi
- current_network_identifier_hash, if available without invasive permission
- schedule_background_sync(constraints)
- cancel_background_sync

Use an in-memory fixture adapter on desktop test targets so the complete Rust sync state machine can be tested without a phone.

### 11.2 Staging on the phone

Do not pass multi-megabyte media through Svelte/JavaScript IPC.

1. Native adapter exports one source resource to an app-private temporary file.
2. While staging, calculate byte count and SHA-256, or let Rust stream the staged file once to calculate them.
3. Store staged path and digest in Pocket ledger.
4. Rust streams that file to Node.
5. Keep it only until the signed receipt is verified and ledger is committed.
6. Delete the staged copy regardless of whether source cleanup is enabled.
7. Bound total staging space; normally stage only the active asset or next small asset.

Cloud-only placeholders are skipped in version 1. The UI says “Original is not stored on this phone” and leaves the source untouched. Do not unexpectedly download from iCloud/Google merely because the phone API can.

### 11.3 Planner

The planner:

1. Confirms paired Node is reachable and authenticated.
2. Confirms Wi-Fi-only constraint.
3. Asks adapter for assets after its durable cursor.
4. Creates/loads opaque asset mappings.
5. Expands each asset into all required resources.
6. Compares current manifest hash with prior receipt.
7. Skips exact previously receipted manifests.
8. Queues new/changed manifests oldest-first by default.
9. Never marks the platform enumeration cursor complete until local queue rows are durable.

Do not infer owner from contents. The server’s paired device record supplies owner and destination.

### 11.4 Worker state machine

Phone states:

    discovered
      -> staging
      -> ready
      -> creating_upload
      -> sending
      -> server_verifying
      -> receipt_received
      -> receipt_verified
      -> cleanup_eligible

Failure states retain enough data to retry:

- waiting_for_node
- waiting_for_wifi
- waiting_for_permission
- waiting_for_space
- retryable_error
- source_changed
- permanent_error

Transitions are stored before presenting progress. On process restart, derive work from the ledger, not an in-memory queue.

Retry policy:

- immediate resume after a transient connection drop
- exponential backoff with jitter, capped at 30 minutes
- reset backoff when Node is rediscovered or user taps Retry
- no retry storm while drive is absent
- permanent errors require a clear action

### 11.5 Background reality

Android:

- Use WorkManager with network type unmetered/local Wi-Fi as closely as platform APIs allow.
- Offer “only while charging” as an optional constraint.
- Run immediate sync when Pocket opens and when Node discovery appears.
- Do not add an always-running foreground service in version 1.
- State honestly that Android may defer work under Doze/battery restrictions and that a force-stopped app cannot run.

iOS:

- Register BGProcessingTask/BGAppRefreshTask identifiers.
- Run immediate sync while Pocket is open.
- Schedule opportunistic background work but never promise a fixed interval.
- Respect local-network and Photos permission states.
- CI validates compilation and simulated behavior; distribution remains out of scope.

Product copy:

> Smriti syncs when your phone and library meet on the same Wi-Fi. Your phone may wait for the operating system to give it background time; opening Pocket syncs immediately.

### 11.6 Cleanup review

After receipt verification:

1. Mark each resource/asset eligible locally.
2. Calculate reclaimable bytes from original platform resources, not staged copies.
3. Show a review page grouped by transfer batch.
4. Default every destructive choice to unselected.
5. Display:

       148 photos safely reached Smriti
       You can review 3.7 GB to remove from this phone

6. Explain that Smriti verified exact bytes on the library drive.
7. Warn about cloud-gallery propagation.
8. User selects items and taps the explicit system-mediated action.
9. Invoke Android trash request or iOS PhotoKit deletion change request.
10. Record only the OS result. If canceled/denied, keep receipt and offer later.

Android must not request MANAGE_EXTERNAL_STORAGE. Use scoped MediaStore/Photo Picker APIs and MediaStore.createTrashRequest where supported.

Cleanup eligibility is asset-wide: a Live Photo is not eligible if only its still image transferred; RAW+JPEG is not eligible if a required member failed.

---

## 12. Read-only phone browsing

### 12.1 Pocket navigation

Keep the mobile surface small:

- Timeline
- Search
- Albums
- People
- Memories
- Sync

Do not copy desktop administration pages into Pocket.

### 12.2 Service reuse

Each Node browse handler:

1. Checks browse_upload capability.
2. Parses/limits DTO.
3. Calls the existing engine repository/service.
4. Maps engine data to a Link DTO.
5. Removes filesystem/internal fields.
6. Returns cursor page.

If command logic currently lives in a Tauri handler, extract the non-Tauri part to src/services or a repository method. Both Tauri and Node call it. Never call a Tauri command from Node.

Semantic search runs on Node with the existing embedding model and index. Pocket sends the text query, filters, and cursor; it receives ranked result metadata and thumbnail IDs. If semantic assets are unavailable, return feature_unavailable with setup guidance. Do not silently degrade a semantic query into misleading filename search.

### 12.3 DTO minimums

Grid item:

- stable photo ID
- media kind
- capture timestamp
- width/height
- favorite flag if already supported
- thumbnail endpoint identity and ETag
- duration for video
- optional coarse place display already computed by Smriti

Photo detail:

- grid fields
- existing non-sensitive metadata shown by desktop
- original availability and size
- read-only album/person relations where already available

Do not return:

- absolute paths
- SQLite IDs that authorize other operations
- face embeddings
- semantic vectors
- model files
- EXIF fields desktop intentionally hides

### 12.4 Cache

- Cache only bounded metadata and thumbnails by default.
- Do not cache originals unless the user explicitly saves/shares one through a native action.
- Key objects by library ID, photo ID, rendition, and ETag.
- Use LRU eviction with a user-visible default cap, initially 500 MiB.
- Store cached objects in app-private storage.
- Add application-level authenticated encryption for cached objects using a random cache key protected by the native secure-key storage adapter.
- Use unique nonces and a maintained AEAD implementation; never reuse a nonce/key pair.
- Decrypt only for display/streaming and avoid persistent plaintext copies.
- Clear a library’s cache on unpair or explicit Clear Cache.

Prefetch only the next small page and nearby visible thumbnails. Cancel offscreen requests. Never prefetch originals.

### 12.5 Performance targets

Measure on a representative USB HDD Node and ordinary 802.11ac home Wi-Fi:

- authenticated reconnect after discovery: median under 500 ms
- cached timeline metadata query on Node: p95 under 150 ms
- first 30-thumbnail grid usable: under 1.5 seconds on the reference LAN after a warm Node start
- scroll fetches do not block UI thread
- upload memory stays bounded independent of file size
- two phones uploading do not starve timeline thumbnail requests

These are engineering targets, not marketing promises. Original RAW/video speed is limited by drive and Wi-Fi throughput. The UI should feel instant for grids because it serves thumbnails and pages, not because it pretends a 4K video is local.

---

## 13. Node runtime and desktop integration

### 13.1 Node controller

smriti-node exposes a NodeController with:

- start
- stop
- status
- begin_pairing
- confirm_pairing
- cancel_pairing
- list_owners
- create_or_select_owner
- list_devices
- revoke_device
- generate_reconnect_locator

start receives a NodeDependencies object containing:

- library root
- database handle
- thumbnail/search/service handles
- library identity
- library lease ownership marker
- cancellation token
- event sink

This makes embedded and headless operation share exactly the same server.

### 13.2 Desktop Tauri commands

Add a focused commands/link.rs and typed client module. Commands are local administrative actions, not photo transport:

- get_link_status
- enable_link
- disable_link
- begin_phone_pairing
- confirm_phone_pairing
- cancel_phone_pairing
- list_link_owners
- create_link_owner
- rename_link_owner
- list_link_devices
- revoke_link_device
- get_reconnect_qr

Long-running state changes emit typed events with job IDs following existing conventions. Update docs/COMMAND_SURFACE.md before wiring UI so names and payloads remain synchronized.

disable_link:

- stops discovery/listener
- invalidates live sessions
- keeps identity, device, provenance, and receipts
- does not remove imported photos
- does not silently revoke phones

reset_link_identity is a separate, highly explicit recovery command:

- stops Link
- revokes all devices
- invalidates old identity
- creates new identity only after confirmation
- keeps imported photos/provenance

### 13.3 Desktop UI

Settings → Phone Sync contains:

- Off/on toggle, default off
- “Only works on your local network; no Smriti server is involved”
- Node status and safe LAN name
- Add phone
- owner list
- authorized-device cards
- capability, last seen, last transfer, revoke
- troubleshooting for firewall, guest Wi-Fi, sleeping host, absent drive
- link to privacy/security explanation

Pairing modal must support keyboard/screen reader use, countdown expiry, QR alternative text/instructions, Cancel, code comparison, and clear failure recovery.

Do not show a QR continuously in Settings. Generate a fresh attempt only after Add phone.

### 13.4 Headless binary

smriti-node serve:

- requires explicit --library or a saved local service config
- validates assets/library and acquires lease
- runs recovery
- starts TLS and mDNS
- emits concise local logs
- exits non-zero if another writer owns the library
- handles Ctrl+C/service stop by ending discovery, rejecting new transfers, flushing DB, and releasing lease

Provide service examples:

- systemd user/system service
- Windows service wrapper instructions
- macOS launchd plist example
- Docker/container example only when the drive is bind-mounted directly and one container owns it

Do not make Docker mandatory. Do not publish a container that runs as root by default. Health checks expose only local process readiness, not library metadata.

### 13.5 Sleep and host changes

- Desktop sleep naturally makes Node unavailable; Pocket waits.
- On wake, Node re-advertises and sessions reauthenticate.
- Moving the drive to a different authorized Smriti host carries identity and paired-device records with it.
- Pocket matches library UUID and pinned public key, not hostname/IP.
- The new host may prompt for its own firewall permission, but the phone does not re-pair.
- If two physical clones advertise the same identity, Pocket shows a duplicate-library warning and refuses automatic upload until the user resolves/reset-identifies one copy.

---

## 14. Platform implementation details

### 14.1 Android

Use Tauri’s supported Android project generation and Kotlin adapter.

Permissions:

- INTERNET for LAN sockets
- ACCESS_NETWORK_STATE and ACCESS_WIFI_STATE as needed for constraints
- multicast lock only while discovering
- READ_MEDIA_IMAGES and READ_MEDIA_VIDEO on supported Android versions
- supported legacy read permission only when required by the minimum SDK
- never MANAGE_EXTERNAL_STORAGE

Behavior:

- Support full and limited/user-selected photo access where Android exposes it.
- Enumerate through MediaStore with stable local cursor data.
- Open content URI streams in Kotlin and stage into app-private cache.
- Preserve original bytes; do not decode/re-encode.
- Use Network Service Discovery/mDNS through the maintained Rust implementation or a narrow Kotlin adapter, but keep one discovery result model.
- Schedule WorkManager jobs and surface OEM battery restrictions as troubleshooting, not as a request for invasive permissions.
- Use system trash request for cleanup.

APK:

- package/application ID is stable from the first public build
- min/target SDK follow supported Tauri/security requirements
- release signed by one long-lived offline-backed key
- application verifies update manifest/checksum before opening Android installer
- Android itself verifies package-signing identity on upgrade
- no Firebase, Play Services, analytics SDK, crash uploader, or Play Integrity dependency

### 14.2 iOS

Use Tauri’s supported Apple project generation and Swift adapter.

Info/entitlements:

- NSPhotoLibraryUsageDescription
- NSPhotoLibraryAddUsageDescription only if a real write/save flow needs it
- NSLocalNetworkUsageDescription
- NSBonjourServices containing _smriti._tcp
- permitted background task identifiers

Behavior:

- Enumerate PHAsset incrementally.
- Handle limited Photos access cleanly.
- Use PHAssetResourceManager to stage original resource bytes.
- Set network access disallowed for cloud-only resources in version 1.
- Preserve Live Photo/resource relationships in the manifest.
- Use PhotoKit performChanges for user-approved cleanup.
- Use Keychain/SecKey for non-exportable signing.
- Use protected app storage for ledger/staging/cache.

CI:

- build Rust iOS targets on macOS
- build an unsigned simulator app
- run Rust unit tests and XCTest adapter tests
- do not attempt to publish an unsigned IPA

Developer documentation may explain Xcode self-build. State that free Personal Team installs expire and are not a public distribution solution.

### 14.3 Shared UI and accessibility

- Touch targets at least 44 by 44 logical points.
- Sync remains understandable with VoiceOver/TalkBack.
- Progress does not depend on color alone.
- Reduced-motion preference disables decorative animation.
- QR screen has written steps.
- Error messages name the next action.
- Dates/sizes use locale-aware formatting.
- Owner/device names are user text and escaped everywhere.

---

## 15. Security threat model

Create docs/link-threat-model.md during implementation and keep this table as its minimum.

| Threat | Required defense/test |
|---|---|
| LAN eavesdropper | TLS for every byte; packet capture contains no readable image/metadata/token. |
| Malicious mDNS advertiser | Match opaque library ID and verify pinned public-key hash before sending credentials/data. |
| Pairing QR photographed by attacker | Five-minute, one-use token plus host confirmation and matching short code. |
| Replay of pairing/session signature | One-use nonce, timestamp window, transcript binding, used-challenge cache. |
| Stolen/lost phone | OS-protected non-exportable key, host revocation, short sessions, private cache protection. |
| Stolen drive | Document that original files and portable identity are physically accessible unless the user encrypts the drive; Link does not claim drive encryption. |
| Compromised paired phone | Capability boundary, quotas, rate limits, revocation, path control, no drive deletion API. |
| Path traversal filename | Treat names as labels, sanitize, canonicalize destination, containment check, adversarial tests. |
| Malicious/huge media | Length/body/time limits, content validation, streaming, decoder isolation through existing pipeline, no request-thread full decode. |
| Disk-fill attack | safety reserve, per-device/node in-flight quotas, stale cleanup. |
| Chunk tampering | TLS plus chunk hash plus final full SHA-256. |
| Receipt forgery | Canonical payload signed by pinned library identity and verified on phone. |
| Premature phone deletion | Cleanup state reachable only from stored, verified complete receipt. |
| Device revoked mid-transfer | Recheck active authorization at chunk/final commit; no receipt. |
| Drive unplugged mid-transfer | I/O error closes availability; no receipt; resumable phone ledger. |
| Two Node writers | OS library lease and single embedded connection. |
| Downgrade | Protocol min/max negotiation included in signed transcript. |
| Token/log leakage | Redaction tests; tokens only in memory; safe structured logs. |
| Timing/key comparison leak | Maintained crypto and constant-time token/hash comparisons. |
| CSRF/browser probing | No cookie auth, strict methods/content types, bearer sessions, no permissive CORS, no browser admin API. |
| Public network exposure | No relay/UPnP; authenticated protocol on selected local interfaces; firewall guidance; Link default off. |
| Duplicate cloned identity | Detect simultaneous duplicate advertisements/endpoints; pause upload and require user reset on the clone. |

Additional requirements:

- Dependency vulnerability audit in CI/release.
- Fuzz/property tests for manifest/path/error parsers.
- No unsafe Rust introduced solely for networking/crypto.
- Structured logs default to INFO without filenames; debug logging is explicit and still redacts secrets.
- Never persist pairing tokens, session tokens, challenges, raw IP history, or private-key bytes in SQLite.
- Obtain an independent security review—community/volunteer review is acceptable for this hobby project—before describing the feature as stable.

---

## 16. Implementation order

Follow this order. Each phase ends with working tests and a small reviewable commit. Do not build all UI first and hope the data path works later.

### Phase 0 — Freeze contracts

Tasks:

1. Add an architecture decision record linking to this plan.
2. Add Link terms and non-goals to docs.
3. Draft protocol DTOs and state diagrams without network code.
4. Confirm names do not collide with existing commands/routes.
5. Record supported Rust/MSRV, Android, iOS, and Node targets.

Exit:

- Maintainer signs off on product boundaries, schema, endpoint names, and receipt semantics.

### Phase 1 — Protocol crate

Tasks:

1. Add smriti-link-protocol.
2. Implement typed IDs and version range.
3. Implement DTOs/error envelope.
4. Implement canonical serialization wrapper.
5. Implement manifest hash and receipt sign/verify helpers.
6. Add golden vectors under tests/fixtures for canonical bytes, SHA-256, public-key pin, signature, and error JSON.
7. Prove Android/iOS-compatible Rust targets compile the crate.

Exit:

- Golden vectors are deterministic across Windows/macOS/Linux.
- No Tauri/database/filesystem dependency appears in cargo tree for this crate.

### Phase 2 — Lease, identity, and migration

Tasks:

1. Add LibraryLease and wire every top-level writer.
2. Add portable identity manager and atomic creation.
3. Add schema v29 and repositories.
4. Add reset/recovery behavior.
5. Test migration/locking/identity corruption.

Exit:

- Existing libraries open with Link off.
- Two writers cannot open one library.
- Moving a test library directory preserves identity.

### Phase 3 — Shared safe ingest

Tasks:

1. Extract reusable primitives from Takeout importer.
2. Add sync_all and parent-sync semantics.
3. Add path and collision adversarial tests on Windows and Unix.
4. Keep Takeout integration tests green.
5. Add injectable free-space/IO fault interfaces for deterministic tests.

Exit:

- A fixture stream is either absent, resumable, or durably present—never reported complete while truncated.

### Phase 4 — Node vertical slice

Tasks:

1. Add TLS identity/pinning.
2. Add loopback-only test server and hello.
3. Add pairing attempt and signed session.
4. Add one-photo manifest/chunk/finalize/receipt.
5. Add restart recovery and idempotent retry.
6. Add revocation.

Exit:

- A fake Pocket pairs, uploads a >16 MiB fixture through several chunks, kills/restarts Node mid-transfer, resumes, receives a valid receipt, and exact bytes/hash match.
- Wrong key/token/certificate tests fail safely.

### Phase 5 — Full multi-resource and concurrent ingest

Tasks:

1. Add Live Photo/RAW/sidecar manifests.
2. Add owner/device destination layout.
3. Add two-phone concurrency and quotas.
4. Add drive-removal/disk-full fault tests.
5. Queue normal scanner/indexing jobs after durable commit.

Exit:

- Two simulated owners can concurrently upload same-named files to separate folders.
- Cleanup receipt is withheld if any required resource fails.

### Phase 6 — Embedded desktop Node

Tasks:

1. Add NodeController to OpenLibrary lifecycle.
2. Add Tauri admin commands and command contract docs.
3. Add Settings UI and pairing/revocation flow.
4. Add mDNS.
5. Add reconnect behavior after sleep/drive move.

Exit:

- Fresh desktop install enables Link in one settings action, pairs through one guided QR flow, and survives app/drive restart without re-pairing.

### Phase 7 — Pocket core and fake adapters

Tasks:

1. Create src-pocket Tauri app and Pocket Vite entry.
2. Add local ledger.
3. Implement discovery/session/client.
4. Implement sync planner/worker against fixture media.
5. Implement verified receipt and cleanup gating.
6. Add core Pocket screens with fake data.

Exit:

- Desktop-hosted fixture Node and desktop-test Pocket core pass the full state machine with no mobile SDK.

### Phase 8 — Android adapter and APK

Tasks:

1. Add MediaStore adapter and permissions.
2. Add Android Keystore signing adapter.
3. Add WorkManager scheduling.
4. Add system trash request.
5. Test physical-device LAN pairing/upload/background/resume.
6. Add signed APK release/update path.

Exit:

- A nontechnical tester downloads the clearly named APK, installs it, scans one QR, uploads originals, browses, and optionally reviews cleanup without command-line work.

### Phase 9 — Read-only browsing

Tasks:

1. Implement paginated timeline/search/album/people/memory service adapters.
2. Add thumbnail/Range endpoints and encrypted bounded cache.
3. Add mobile navigation and loading/error states.
4. Benchmark HDD/LAN behavior and tune bounded concurrency.

Exit:

- Grids meet reference targets and an upload cannot make browsing unusable.
- Semantic search results match desktop for the same query/library.

### Phase 10 — Headless Node

Tasks:

1. Add CLI and OS-local admin IPC.
2. Add service packaging/examples.
3. Build/test Windows, macOS, Linux x64, Linux ARM64.
4. Test moving an already-paired drive from desktop to headless Node.

Exit:

- Phone reconnects without re-pairing and resumes an interrupted transfer.
- CLI pairing/revocation works without opening a second database.

### Phase 11 — iOS implementation and CI

Tasks:

1. Add PhotoKit resource adapter.
2. Add Keychain/SecKey adapter.
3. Add local-network discovery/permissions.
4. Add background task integration.
5. Add PhotoKit cleanup request.
6. Compile/test simulator in macOS CI.

Exit:

- Shared Pocket core and Swift adapter tests pass on simulator.
- No release documentation implies a publicly installable IPA.

### Phase 12 — Hardening and release candidate

Tasks:

1. Run full fault, threat, performance, upgrade, and accessibility matrix.
2. Audit dependencies and permissions.
3. Complete privacy/security/user docs.
4. Run external security review and resolve high/critical findings.
5. Test release artifacts from a clean machine and clean phone.
6. Mark protocol v1 stable only after all acceptance checks pass.

Exit:

- Every checkbox in sections 18–21 is satisfied with recorded evidence.

---

## 17. Exact test plan

### 17.1 Unit tests

Protocol:

- canonical JSON remains byte-identical
- field-order variation yields same manifest hash
- changed resource changes manifest hash
- malformed IDs/hashes/sizes rejected
- protocol negotiation edges
- receipt signature success/failure
- public-key pin vector

Paths:

- ../ and encoded traversal
- slash/backslash
- absolute paths and drive prefixes
- Windows CON, PRN, AUX, NUL, COM1, LPT1
- trailing spaces/dots
- Unicode normalization/collisions
- empty/very long names
- duplicate names and suffixes
- final containment after symlink/canonicalization checks

Auth:

- valid signature
- wrong device/library/challenge/version
- expired/replayed challenge
- revoked device
- expired session
- capability denial
- constant-time token comparison wrapper

State machines:

- every allowed transition
- every illegal transition
- process restart at every transition
- receipt is the sole cleanup-eligibility predecessor

### 17.2 Engine/database integration tests

- v28 to v29 migration
- fresh v29 schema equality
- migration rollback fault
- owner/device slug collision
- revocation preserves provenance
- durable transaction/recovery
- LibraryLease concurrency
- receipt file/DB reconciliation
- scanner accepts final Link import
- Google Takeout behavior unchanged

### 17.3 Node end-to-end tests

Use temp libraries and real loopback TLS:

1. pair and session
2. upload empty-invalid, tiny, 4 MiB boundary, >16 MiB, and large streamed fixtures
3. disconnect after every chunk
4. restart Node after every server state
5. retry complete repeatedly
6. tamper chunk, staged file, receipt, certificate
7. two phones, two owners, same filename
8. same phone exact retry
9. different phones identical hash preserved separately
10. device revoked before chunk and before completion
11. token expiration and rate limiting
12. disk-full/free-space rejection
13. simulated drive removal
14. malicious header/body lengths and slow body timeout
15. browse forbidden for upload-only device
16. cursor boundary/stability
17. Range/ETag correctness

### 17.4 Pocket core tests

With fake native adapter and fake clock/network:

- first discovery and pairing
- Node IP/host change
- Wi-Fi disappears/reappears
- app killed at each state
- source changes during staging
- limited photo permission
- cloud-only unavailable item
- Live Photo partial failure
- receipt verification failure
- cleanup setting off
- cleanup review cancellation
- OS deletion denied/partial success
- cache eviction and corruption
- revoked device

### 17.5 Platform tests

Android:

- supported minimum/current API levels
- full/limited permission
- JPEG, HEIC, PNG, RAW, MP4, Live/Motion resource where available
- screen locked/background/Doze
- WorkManager retry
- scoped trash request
- APK upgrade preserves key/ledger
- wrong signing-key APK cannot upgrade

iOS simulator/unit:

- PhotoKit permission states
- limited-library changes
- resource enumeration/manifests
- Keychain signing
- local-network permission denial
- background task handoff
- delete request cancellation

Physical iPhone validation is required before any future distributable build even though simulator CI exists.

### 17.6 Manual fault matrix

During a large transfer:

- unplug drive
- kill Pocket
- kill Node
- close desktop
- sleep/wake phone
- sleep/wake host
- disable Wi-Fi
- change host IP
- move drive to a second host
- fill drive to reserve threshold
- revoke sending phone

For each, record:

- what user sees
- whether source remains
- server/phone ledger state
- recovery path
- final hash
- whether cleanup became eligible

It is a release blocker if any failure can show verified success without a valid durable receipt.

---

## 18. CI and pre-commit work

### 18.1 Local scripts

Keep scripts/ci_local.sh ci as the required desktop pre-push gate. Extend or add focused modes:

- ci-node: protocol, Node, headless CLI checks/tests
- ci-pocket-core: Pocket Rust and Svelte check/tests without mobile SDK
- ci-mobile-android: Android compile/unit checks when SDK is installed

Update the PowerShell equivalent where one exists. Do not make a developer download Xcode on Windows/Linux merely to run normal local CI.

### 18.2 GitHub Actions

Existing quality workflow:

- add protocol/Node/Pocket-core formatting, clippy, and tests to all appropriate OS jobs
- preserve current frontend check/test/build
- cache one target tree per real platform; do not create uncontrolled build caches

Android job:

- pinned Java/Android/Rust toolchains
- cargo mobile target checks
- Gradle/Kotlin unit tests
- debug APK build on pull requests
- no signing secret exposed to pull requests/forks

iOS job on macos-latest:

- install required Rust Apple targets
- build shared core
- build unsigned simulator app
- run Swift/XCTest tests
- no signing/provisioning secret required

Security job:

- dependency audit
- license/policy check
- secret scan
- protocol fixture/golden-vector check

### 18.3 Build hygiene

Respect repository disk policy:

- use the normal target directory
- run clean_builds after roughly three Rust build/check invocations
- retain useful deps
- remove any temporary mobile/isolated target directory created by the implementation
- keep only current release bundles

---

## 19. Release and deployment

### 19.1 Artifacts

Tag workflow publishes:

- existing Smriti desktop bundles
- Smriti-Node-Windows-x64.zip
- Smriti-Node-macOS-arm64.tar.gz
- Smriti-Node-macOS-x64.tar.gz if supported
- Smriti-Node-Linux-x64.tar.gz
- Smriti-Node-Linux-arm64.tar.gz
- Smriti-Pocket-Android.apk
- versioned copies of each
- SHA256SUMS covering all artifacts

Do not publish an iOS IPA.

### 19.2 Android signing

1. Generate one production application-signing key offline.
2. Store two encrypted offline backups under maintainer control.
3. Put the CI copy/passwords only in GitHub encrypted secrets protected by the release environment.
4. Never expose secrets to PR workflows or logs.
5. Keep application ID and signing key stable forever; losing it prevents seamless upgrades.
6. After build, verify APK signature, package ID, version, and checksum.
7. Install the released APK over the previous release on a clean physical test device.

### 19.3 Android update UX

- Update checks are separate from Link and require internet.
- Follow the existing opt-in GitHub Release/checksum model.
- Clearly label the universal stable APK so nontechnical users do not choose architectures.
- Pocket downloads only after user approval.
- Verify SHA256SUMS and expected release metadata.
- Hand the verified APK to Android’s installer; never claim silent install.
- Explain the one-time “install unknown apps from this source” setting.
- Do not redirect users to a confusing raw asset list from the main download call-to-action.

### 19.4 Headless installation

Provide:

- one clearly named archive per OS/architecture
- checksum
- install/start guide
- service templates
- asset setup integration
- update command/check with explicit confirmation

The normal desktop user needs none of this: enabling Phone Sync starts embedded Node.

### 19.5 Compatibility policy

- Protocol v1 remains accepted for at least one normal Pocket/Desktop release overlap.
- hello reports min/max.
- Release notes say when an update is required before transfer.
- Database migrations remain forward-only and backed up under existing Smriti policy.
- A newer Pocket never sends a v2 field that changes v1 signature meaning without a protocol bump.

---

## 20. Documentation changes required with implementation

Update these before release:

### README

- Link remains off by default.
- Same-LAN transfer/browser capability.
- no cloud/relay/account
- Android APK link
- honest iOS status
- embedded desktop versus headless Node

### PRIVACY.md

- exact network connections Link makes
- mDNS metadata
- what lives on phone/drive
- no telemetry/cloud
- optional update-check internet traffic
- threat boundary for physical drive and compromised devices

### SECURITY.md

- protocol/security overview
- report process
- supported versions
- lost phone/revocation
- lost/corrupt identity
- external review status

### docs/COMMAND_SURFACE.md

- local Tauri administration commands only
- explicit note that /link/v1 is a separate authenticated protocol

### New user guide: docs/phone-sync.md

- enable
- pair
- owner/device folders
- sync timing
- browse
- verified cleanup
- revoke
- move drive to another host
- guest Wi-Fi/firewall/sleep troubleshooting
- no “backup” overclaim

### New technical docs

- docs/link-protocol-v1.md
- docs/link-threat-model.md
- docs/link-receipts-and-recovery.md
- docs/headless-node.md
- docs/pocket-android-build.md
- docs/pocket-ios-build.md

All diagrams/copy must preserve the core message: photos travel directly between the phone and the user-controlled Node on the local network.

---

## 21. Definition of done

The feature is not done when a demo uploads one JPEG. It is done only when all of the following are true.

### Product

- [ ] Link is off by default.
- [ ] Desktop enable and pairing are guided and require no terminal.
- [ ] Android install has one obvious APK choice.
- [ ] Multiple phones pair independently and map to chosen owners.
- [ ] Drive move to another Node host does not require re-pair.
- [ ] Phone browsing covers timeline, semantic search, albums, people, and memories read-only.
- [ ] No cloud/account/store dependency was introduced.
- [ ] iOS code compiles/tests without pretending public distribution exists.

### Data safety

- [ ] Originals are transferred byte-for-byte.
- [ ] Full SHA-256 is verified on Node.
- [ ] Required multi-resource assets are atomic for cleanup eligibility.
- [ ] Final file and receipt survive power/process failure tests.
- [ ] Retries are idempotent.
- [ ] Disk full, drive removal, and database failure cannot produce success.
- [ ] Existing Google Takeout importer remains correct.
- [ ] Cleanup is off by default and always system/user confirmed.

### Security

- [ ] TLS public key is pinned from QR.
- [ ] Every phone has a unique non-exportable OS-protected key.
- [ ] Pair tokens are one-use, memory-only, and expire.
- [ ] Session challenges cannot replay.
- [ ] Revocation takes effect immediately.
- [ ] Capability checks cover every endpoint.
- [ ] Limits/path validation/adversarial tests pass.
- [ ] Logs and diagnostics contain no secrets.
- [ ] No UPnP/public relay/anonymous route exists.
- [ ] Threat model and external review have no unresolved high/critical finding.

### Reliability and performance

- [ ] Phone ledger resumes after process death.
- [ ] Node recovery is idempotent at every state.
- [ ] Single-writer lease works on supported OSes.
- [ ] Two phones can upload without corrupting state.
- [ ] Browsing remains responsive during upload.
- [ ] Reference LAN targets are measured and recorded.
- [ ] Background behavior is honest and tested within OS limits.

### Engineering/release

- [ ] Root workspace, lockfile, docs, and command contract are updated.
- [ ] Rust fmt/clippy/tests pass.
- [ ] Svelte check/tests/build pass for desktop and Pocket.
- [ ] Windows/Linux/macOS Node checks pass.
- [ ] Android debug/release checks pass.
- [ ] iOS simulator build/tests pass in macOS CI.
- [ ] Release APK signature and upgrade path are tested.
- [ ] All published artifacts have checksums.
- [ ] scripts/ci_local.sh ci passes before merge/push.

---

## 22. Final walkthrough to use as the release test

Perform this exact human test from clean state:

1. Install current Smriti desktop release on a clean computer.
2. Attach a library drive and open it.
3. Confirm no Link listener exists before enabling Phone Sync.
4. Enable Phone Sync from Settings.
5. Install the clearly named Smriti Pocket APK on an Android phone.
6. Choose “Add phone,” create owner “Asha,” name device “Asha’s Phone,” and choose browse and upload.
7. Scan QR and compare the six-digit code.
8. Take a JPEG, a video, and a multi-resource photo if device supports it.
9. Put phone/host on the same ordinary Wi-Fi.
10. Confirm Pocket uploads without internet and shows honest progress.
11. Unplug the drive halfway through the largest item. Confirm no success/cleanup appears.
12. Attach the drive to a second Smriti host with Node enabled.
13. Confirm Pocket rediscovers the same library without pairing and resumes.
14. Verify final files appear under Asha’s immutable owner/device folders.
15. Compare every source/final SHA-256.
16. Confirm desktop indexes the photos.
17. Search semantically from Pocket and compare results to desktop.
18. Browse timeline, album, person, and memory grids.
19. Turn on “Offer to free phone space after verified transfer.”
20. Confirm only fully receipted assets are offered, with exact reclaimable size and cloud-gallery warning.
21. Cancel the OS deletion once; confirm nothing is marked deleted.
22. Approve it the second time; confirm drive files remain.
23. Pair a second owner/device and upload a file with the same name; confirm separation.
24. Revoke the first phone; confirm browse/upload stop immediately while the second phone continues.
25. Disable Link; confirm listener/discovery stop and imported photos remain.
26. Re-enable Link; confirm non-revoked phone reconnects.
27. Run all CI/release gates and install the actual release artifact, not a development build.

Any discrepancy is a bug to resolve before calling Smriti Link complete.

---

## 23. The guardrail for future remote access

Version 1 ends at same-LAN operation. To keep that future option clean, define one client dialing boundary now:

    trait LinkTransport {
        discover_or_resolve_library(...)
        connect_pinned_tls(...)
    }

LAN uses mDNS plus direct TCP. A future tunnel may resolve a secure route, but everything above the connected, pinned stream remains unchanged:

- same library identity
- same device identity
- same challenge/session
- same capability
- same endpoints
- same upload hashes
- same durable receipt
- same cleanup gate

Do not add tunnel code, accounts, relay configuration, or remote UI in this implementation.

---

## 24. First action for the implementing engineer

Before writing production code:

1. Read this entire document.
2. Read AGENTS.md, docs/COMMAND_SURFACE.md, current schema/migrations, OpenLibrary/AppState, scanner, ThumbnailService, semantic search, and Google Takeout importer.
3. Create a checklist issue/working plan mirroring phases 0–12.
4. Implement only Phase 0 and Phase 1 first.
5. Keep changes small and tests focused during development.
6. Run the repository cleanup script after roughly every three Rust build/check invocations.
7. Never weaken a receipt, lease, pairing, or cleanup invariant to make a demo pass.

The intended result is deliberately modest in surface area and unusually strong underneath: one QR, the same Wi-Fi, the user’s own drive, independently trusted phones, exact original bytes, and no claim of success until Smriti can prove those bytes are safely there.
