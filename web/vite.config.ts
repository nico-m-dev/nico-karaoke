import { defineConfig, type Plugin } from "vite";

/** Serve the Studio at `/editor`, matching the production server route during development. */
function studioRoute(): Plugin {
  return {
    name: "studio-route",
    configureServer(server) {
      server.middlewares.use((request, _response, next) => {
        const httpRequest = request as { url?: string };
        if (!httpRequest.url) {
          next();
          return;
        }
        const [pathname, search] = httpRequest.url.split("?");
        if (pathname === "/editor" || pathname === "/editor/") {
          httpRequest.url = `/editor.html${search ? `?${search}` : ""}`;
        }
        next();
      });
    },
  };
}

export default defineConfig({
  appType: "mpa",
  plugins: [studioRoute()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8770",
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: "index.html",
        editor: "editor.html",
      },
    },
  },
});
