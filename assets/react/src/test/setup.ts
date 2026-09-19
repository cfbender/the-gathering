// Node 25+ defines a `localStorage` getter on globalThis that returns undefined
// unless Node is started with --localstorage-file. Vitest's jsdom environment
// only copies window keys that are *not* already on the Node global, so jsdom's
// real Storage never lands on `localStorage`. Provide an in-memory one here.
class MemoryStorage implements Storage {
  #items = new Map<string, string>()

  get length() {
    return this.#items.size
  }

  key(index: number) {
    return [...this.#items.keys()][index] ?? null
  }

  getItem(key: string) {
    return this.#items.get(key) ?? null
  }

  setItem(key: string, value: string) {
    this.#items.set(key, String(value))
  }

  removeItem(key: string) {
    this.#items.delete(key)
  }

  clear() {
    this.#items.clear()
  }
}

for (const name of ["localStorage", "sessionStorage"] as const) {
  if (globalThis[name] === undefined) {
    Object.defineProperty(globalThis, name, {
      value: new MemoryStorage(),
      configurable: true,
      writable: true,
    })
  }
}
