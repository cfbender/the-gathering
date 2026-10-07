import tailwindcss from "@tailwindcss/vite"
import { tanstackRouter } from "@tanstack/router-plugin/vite"
import react from "@vitejs/plugin-react"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vite-plus"

const backendOrigin = `http://127.0.0.1:${process.env.PORT || "4000"}`
// VITE_PORT moves the dev server off 5173, for example beside another app's in the same orb.
const vitePort = Number(process.env.VITE_PORT || 5173)

// Paths the Vite dev server owns. Everything else (the SPA shell, /api, static
// files under priv/static) is proxied to the backend so one origin serves it all.
const vitePathPrefixes = ["/assets/react/", "/@", "/node_modules/", "/__vite"]

export default defineConfig({
  base: process.env.NODE_ENV === "production" ? "/assets/react/" : "/",
  fmt: {
    semi: false,
    printWidth: 100,
    ignorePatterns: [
      "aube-lock.yaml",
      "assets/react/src/routeTree.gen.ts",
      "priv/static/**",
      "rust/**",
      "*.md",
    ],
  },
  lint: {
    ignorePatterns: ["assets/react/src/routeTree.gen.ts", "priv/static/**", "rust/**"],
    options: { typeAware: true, typeCheck: true },
  },
  test: {
    environment: "jsdom",
    // Each file still gets its own isolated context, but every worker builds jsdom once
    // instead of once per file, which was over half of the suite's run time.
    pool: "vmThreads",
    include: ["assets/react/src/**/*.test.{ts,tsx}"],
    setupFiles: ["assets/react/src/test/setup.ts"],
  },
  plugins: [
    tanstackRouter({
      target: "react",
      routesDirectory: "assets/react/src/routes",
      generatedRouteTree: "assets/react/src/routeTree.gen.ts",
      autoCodeSplitting: true,
      quoteStyle: "double",
      semicolons: false,
    }),
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./assets/react/src", import.meta.url)) },
  },
  build: {
    emptyOutDir: true,
    manifest: true,
    outDir: "priv/static/assets/react",
    rolldownOptions: {
      input: "assets/react/src/main.tsx",
    },
  },
  server: {
    host: "127.0.0.1",
    port: vitePort,
    strictPort: true,
    allowedHosts: [".onamp.dev"],
    // Dedicated workers (the card recognizer and onnxruntime's pthreads) only start inside the
    // cross-origin-isolated webcam table when their scripts carry COEP too. Applies to files
    // Vite serves, not to responses proxied from the backend.
    headers: { "Cross-Origin-Embedder-Policy": "require-corp" },
    proxy: {
      "/socket": {
        target: backendOrigin,
        ws: true,
      },
      "^/.*": {
        target: backendOrigin,
        headers: { "x-the-gathering-vite-proxy": "1" },
        bypass(req) {
          const url = req.url ?? "/"
          if (vitePathPrefixes.some((prefix) => url.startsWith(prefix))) return url
          return null
        },
      },
    },
  },
})
