import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import fs from "node:fs";
import path from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// MIME types for Live2D assets served by the dev middleware below.
const LIVE2D_MIME: Record<string, string> = {
  ".js": "text/javascript",
  ".json": "application/json",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".moc3": "application/octet-stream",
  ".physics3.json": "application/json",
};

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [
    react(),
    tailwindcss(),
    {
      // In dev mode, the Tauri asset:// protocol is blocked by WKWebView CORS
      // (page origin is http://localhost:1420, request is asset://localhost).
      // This middleware serves external Live2D files over HTTP so fetch/<script>
      // work. The path is the filesystem path, URI-encoded with encodeURI (slashes
      // kept intact so relative resource references in model3.json resolve correctly).
      name: "live2d-dev-server",
      configureServer(server) {
        const handler: any = (req: any, res: any, next: any) => {
          if (!req.url?.startsWith("/__live2d__/")) return next();
          const url = new URL(req.url, "http://localhost");
          const filePath = decodeURIComponent(url.pathname.slice("/__live2d__/".length));
          const ext = path.extname(filePath).toLowerCase();
          const mime = LIVE2D_MIME[ext] || "application/octet-stream";
          fs.readFile(filePath, (err, data) => {
            if (err) {
              res.statusCode = 404;
              res.setHeader("Content-Type", "text/plain");
              res.end(`Not found: ${filePath}`);
              return;
            }
            res.setHeader("Content-Type", mime);
            res.setHeader("Access-Control-Allow-Origin", "*");
            res.end(data);
          });
        };
        server.middlewares.use(handler);
      },
    },
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },

  },
}));
