import { expect, it } from "vite-plus/test"
import { createImageQueue } from "./seat-decklists"

function deferredLoader() {
  const started: string[] = []
  const finish = new Map<string, () => void>()
  const load = (src: string) =>
    new Promise<void>((resolve) => {
      started.push(src)
      finish.set(src, resolve)
    })
  return { started, finish, load }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

it("loads at most `concurrency` images at a time, in order", async () => {
  const loader = deferredLoader()
  const queue = createImageQueue(loader.load, 2)
  queue.add(["a", "b", "c", "d"])
  expect(loader.started).toEqual(["a", "b"])

  loader.finish.get("a")?.()
  await flush()
  expect(loader.started).toEqual(["a", "b", "c"])

  loader.finish.get("b")?.()
  loader.finish.get("c")?.()
  await flush()
  expect(loader.started).toEqual(["a", "b", "c", "d"])
})

it("loads each URL once and skips missing ones", () => {
  const loader = deferredLoader()
  const queue = createImageQueue(loader.load, 10)
  queue.add(["a", undefined, "a", "b"])
  queue.add(["b", "c"])
  expect(loader.started).toEqual(["a", "b", "c"])
})

it("keeps going after a failed image", async () => {
  const started: string[] = []
  const queue = createImageQueue((src) => {
    started.push(src)
    return src === "bad" ? Promise.reject(new Error("404")) : Promise.resolve()
  }, 1)
  queue.add(["bad", "good"])
  await flush()
  expect(started).toEqual(["bad", "good"])
})
