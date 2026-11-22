// Cross-checks the Rust E2EE port against WebCrypto, using the same code paths as the web app
// (utils/e2ee.ts, vault.ts, e2eeTrust.ts):
//
//   node scripts/webcrypto_vectors.mjs make  > tests/fixtures/webcrypto.json   (Node → Rust)
//   node scripts/webcrypto_vectors.mjs check <rust.json>                       (Rust → Node)
//
// The fixture holds throwaway test keys only.
import { readFileSync } from 'node:fs';

const subtle = globalThis.crypto.subtle;
const enc = new TextEncoder();
const dec = new TextDecoder();
const b64 = (buf) => Buffer.from(new Uint8Array(buf)).toString('base64');
const unb64 = (s) => new Uint8Array(Buffer.from(s, 'base64'));
const rnd = (n) => globalThis.crypto.getRandomValues(new Uint8Array(n));
const ECDH = { name: 'ECDH', namedCurve: 'P-256' };

const keyIdOf = async (k) => b64(await subtle.digest('SHA-256', unb64(k))).slice(0, 12);

async function encryptSymmetric(plaintext, k) {
    const key = await subtle.importKey('raw', unb64(k), { name: 'AES-GCM' }, false, ['encrypt']);
    const iv = rnd(12);
    const data = await subtle.encrypt({ name: 'AES-GCM', iv }, key, enc.encode(plaintext));
    return JSON.stringify({ iv: b64(iv), data: b64(data), k: await keyIdOf(k) });
}
async function decryptWithKey(parsed, k) {
    const key = await subtle.importKey('raw', unb64(k), { name: 'AES-GCM' }, false, ['decrypt']);
    return dec.decode(await subtle.decrypt({ name: 'AES-GCM', iv: unb64(parsed.iv) }, key, unb64(parsed.data)));
}
async function shared(privJwk, pubJwk, usage) {
    const priv = await subtle.importKey('jwk', JSON.parse(privJwk), ECDH, false, ['deriveKey', 'deriveBits']);
    const pub = await subtle.importKey('jwk', JSON.parse(pubJwk), ECDH, false, []);
    return subtle.deriveKey({ name: 'ECDH', public: pub }, priv, { name: 'AES-GCM', length: 256 }, false, [usage]);
}
async function createEnvelope(roomKey, myPriv, theirPub) {
    const key = await shared(myPriv, theirPub, 'encrypt');
    const iv = rnd(12);
    const data = await subtle.encrypt({ name: 'AES-GCM', iv }, key, enc.encode(roomKey));
    return JSON.stringify({ iv: b64(iv), data: b64(data) });
}
async function openEnvelope(envelope, myPriv, wrapperPub) {
    const p = JSON.parse(envelope);
    const key = await shared(myPriv, wrapperPub, 'decrypt');
    return dec.decode(await subtle.decrypt({ name: 'AES-GCM', iv: unb64(p.iv) }, key, unb64(p.data)));
}
async function vaultKey(password, salt, iterations) {
    const pk = await subtle.importKey('raw', enc.encode(password), { name: 'PBKDF2' }, false, ['deriveKey']);
    return subtle.deriveKey({ name: 'PBKDF2', salt, iterations, hash: 'SHA-256' }, pk, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}
async function encryptVault(data, password, iterations = 600000) {
    const salt = rnd(16), iv = rnd(12);
    const cipher = await subtle.encrypt({ name: 'AES-GCM', iv }, await vaultKey(password, salt, iterations), enc.encode(data));
    const v = { salt: b64(salt), iv: b64(iv), cipher: b64(cipher) };
    if (iterations === 600000) v.iter = 600000;
    return v;
}
async function decryptVault(v, password) {
    const key = await vaultKey(password, unb64(v.salt), v.iter ?? 100000);
    return dec.decode(await subtle.decrypt({ name: 'AES-GCM', iv: unb64(v.iv) }, key, unb64(v.cipher)));
}
async function fingerprint(jwkString) {
    const j = JSON.parse(jwkString);
    const hex = Buffer.from(await subtle.digest('SHA-256', enc.encode(`${j.crv || 'P-256'}|${j.x}|${j.y}`))).toString('hex').toUpperCase();
    return hex.match(/.{4}/g).join(' ');
}
async function keypair() {
    const kp = await subtle.generateKey(ECDH, true, ['deriveKey', 'deriveBits']);
    return { pub: JSON.stringify(await subtle.exportKey('jwk', kp.publicKey)), priv: JSON.stringify(await subtle.exportKey('jwk', kp.privateKey)) };
}

const mode = process.argv[2];
if (mode === 'make') {
    const roomKey = b64(rnd(32));
    const oldKeys = [b64(rnd(32)), b64(rnd(32))];
    const alice = await keypair(), bob = await keypair();
    const text = 'Привет 👋 hello — E2EE';
    const attachment = { type: 'image', file_name: 'cat.png', size: 42 };
    console.log(JSON.stringify({
        room_key: roomKey,
        key_id: await keyIdOf(roomKey),
        text,
        ciphertext: await encryptSymmetric(text, roomKey),
        old_keys: oldKeys,
        ciphertext_old_key: await encryptSymmetric('from before the rotation', oldKeys[1]),
        archived_keys: await encryptSymmetric(JSON.stringify(oldKeys), roomKey),
        attachment,
        attachment_sealed: { encrypted: true, ...JSON.parse(await encryptSymmetric(JSON.stringify(attachment), roomKey)) },
        alice, bob,
        envelope_alice_to_bob: await createEnvelope(roomKey, alice.priv, bob.pub),
        alice_fingerprint: await fingerprint(alice.pub),
        vault_password: 'correct horse battery',
        vault: await encryptVault('{"priv":"x","pub":"y"}', 'correct horse battery'),
        vault_legacy: await encryptVault('legacy data', 'correct horse battery', 100000),
    }, null, 2));
} else if (mode === 'check') {
    const r = JSON.parse(readFileSync(process.argv[3], 'utf8'));
    const fx = JSON.parse(readFileSync(new URL('../tests/fixtures/webcrypto.json', import.meta.url), 'utf8'));
    const results = {
        symmetric: (await decryptWithKey(JSON.parse(r.ciphertext), r.room_key)) === r.text,
        key_id: (await keyIdOf(r.room_key)) === JSON.parse(r.ciphertext).k,
        envelope_rust_to_bob: (await openEnvelope(r.envelope_rust_to_bob, fx.bob.priv, r.rust.pub)) === r.room_key,
        rust_jwk_imports: !!(await subtle.importKey('jwk', JSON.parse(r.rust.priv), ECDH, false, ['deriveBits'])),
        vault: (await decryptVault(r.vault, r.vault_password)) === r.vault_plain,
        fingerprint: (await fingerprint(r.rust.pub)) === r.rust_fingerprint,
        archived: JSON.stringify(JSON.parse(await decryptWithKey(JSON.parse(r.archived_keys), r.room_key))) === JSON.stringify(r.old_keys),
    };
    console.log(JSON.stringify(results));
    process.exit(Object.values(results).every(Boolean) ? 0 : 1);
}
