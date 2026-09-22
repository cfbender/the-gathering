import tailwindcss from "@tailwindcss/vite"
import { tanstackRouter } from "@tanstack/router-plugin/vite"
import react from "@vitejs/plugin-react"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vite-plus"

const phoenixOrigin = `http://127.0.0.1:${process.env.PORT || "4000"}`

// Paths the Vite dev server owns. Everything else (the SPA shell, /api, static
// files under priv/static) is proxied to Phoenix so one origin serves it all.
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
      "ml/**",
      "deps/**",
      "_build/**",
      "*.md",
    ],
  },
  lint: {
    ignorePatterns: [
      "assets/react/src/routeTree.gen.ts",
      "priv/static/**",
      "ml/**",
      "deps/**",
      "_build/**",
    ],
    options: { typeAware: true, typeCheck: true },
  },
  test: {
    environment: "jsdom",
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
    rollupOptions: {
      input: "assets/react/src/main.tsx",
    },
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    allowedHosts: [".onamp.dev"],
    proxy: {
      "/socket": {
        target: phoenixOrigin,
        ws: true,
      },
      "^/.*": {
        target: phoenixOrigin,
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
