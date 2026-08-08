/**
 * 会话层传输：ECDH P-256（SPKI）+ HKDF-SHA256(info=schema-sync-v1) + AES-256-GCM。
 * 公钥为 Web Crypto exportKey('spki') 的 base64，与后端 cryptography SPKI DER 对齐。
 * HKDF salt 使用 32 字节全零，匹配 cryptography salt=None。
 */

const HKDF_INFO = new TextEncoder().encode('schema-sync-v1')
const ZERO_SALT = new Uint8Array(32)

let sessionId: string | null = null
let aesKey: CryptoKey | null = null
let handshakePromise: Promise<void> | null = null

function b64encode(buf: ArrayBuffer | Uint8Array): string {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf)
  let s = ''
  for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]!)
  return btoa(s)
}

function b64decode(text: string): Uint8Array {
  const bin = atob(text)
  const out = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i)
  return out
}

async function ensureSession(): Promise<void> {
  if (sessionId && aesKey) return
  if (!handshakePromise) {
    handshakePromise = (async () => {
      const pair = await crypto.subtle.generateKey(
        { name: 'ECDH', namedCurve: 'P-256' },
        true,
        ['deriveBits'],
      )
      const spki = await crypto.subtle.exportKey('spki', pair.publicKey)
      const res = await fetch('/api/session/handshake', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ client_public: b64encode(spki) }),
      })
      if (!res.ok) {
        const t = await res.text()
        throw new Error(`会话握手失败：${t || res.statusText}`)
      }
      const body = (await res.json()) as { session_id: string; server_public: string }
      const serverKey = await crypto.subtle.importKey(
        'spki',
        b64decode(body.server_public),
        { name: 'ECDH', namedCurve: 'P-256' },
        false,
        [],
      )
      const sharedBits = await crypto.subtle.deriveBits(
        { name: 'ECDH', public: serverKey },
        pair.privateKey,
        256,
      )
      const hkdfBase = await crypto.subtle.importKey('raw', sharedBits, 'HKDF', false, [
        'deriveKey',
      ])
      aesKey = await crypto.subtle.deriveKey(
        {
          name: 'HKDF',
          hash: 'SHA-256',
          salt: ZERO_SALT,
          info: HKDF_INFO,
        },
        hkdfBase,
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt', 'decrypt'],
      )
      sessionId = body.session_id
    })().finally(() => {
      if (!sessionId) handshakePromise = null
    })
  }
  await handshakePromise
}

async function encryptPayload(obj: unknown): Promise<{
  v: number
  nonce: string
  ciphertext: string
}> {
  await ensureSession()
  if (!aesKey) throw new Error('会话未就绪')
  const nonce = crypto.getRandomValues(new Uint8Array(12))
  const pt = new TextEncoder().encode(JSON.stringify(obj))
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv: nonce }, aesKey, pt)
  return {
    v: 1,
    nonce: b64encode(nonce),
    ciphertext: b64encode(ct),
  }
}

async function decryptEnvelope(envelope: {
  v?: number
  nonce: string
  ciphertext: string
}): Promise<unknown> {
  await ensureSession()
  if (!aesKey) throw new Error('会话未就绪')
  const nonce = b64decode(envelope.nonce)
  const ct = b64decode(envelope.ciphertext)
  const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce }, aesKey, ct)
  const text = new TextDecoder().decode(pt)
  if (!text || text === 'null') return null
  return JSON.parse(text)
}

export async function secureRequest<T>(path: string, init?: RequestInit): Promise<T> {
  await ensureSession()
  if (!sessionId) throw new Error('会话未就绪')

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'X-Schema-Sync-Session': sessionId,
  }
  if (init?.headers) {
    const h = init.headers
    if (h instanceof Headers) {
      h.forEach((v, k) => {
        headers[k] = v
      })
    } else if (Array.isArray(h)) {
      for (const [k, v] of h) headers[k] = v
    } else {
      Object.assign(headers, h)
    }
  }

  let body = init?.body
  if (body != null && typeof body === 'string') {
    const parsed = JSON.parse(body) as unknown
    body = JSON.stringify(await encryptPayload(parsed))
  }

  const res = await fetch(path, { ...init, headers, body })
  const envelope = (await res.json()) as {
    v?: number
    nonce?: string
    ciphertext?: string
    detail?: unknown
  }

  // 未加密的错误（握手前/会话缺失时中间件直出）
  if (envelope.nonce == null || envelope.ciphertext == null) {
    if (!res.ok) {
      const detail =
        typeof envelope.detail === 'string'
          ? envelope.detail
          : envelope.detail != null
            ? JSON.stringify(envelope.detail)
            : res.statusText
      throw new Error(detail || `HTTP ${res.status}`)
    }
    return envelope as T
  }

  const plain = (await decryptEnvelope({
    v: envelope.v,
    nonce: envelope.nonce,
    ciphertext: envelope.ciphertext,
  })) as { detail?: unknown } | T

  if (!res.ok) {
    const detailObj = plain as { detail?: unknown }
    const detail =
      typeof detailObj?.detail === 'string'
        ? detailObj.detail
        : detailObj?.detail != null
          ? JSON.stringify(detailObj.detail)
          : res.statusText
    // 会话失效时重置，便于下次重握
    if (res.status === 401) {
      sessionId = null
      aesKey = null
      handshakePromise = null
    }
    throw new Error(detail || `HTTP ${res.status}`)
  }

  return plain as T
}
