import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { applyPreferences, readPreferences } from "./preferences/preferences";
import { PresentationProvider } from "./preferences/PresentationProvider";
import "./styles/index.css";

// Before the first paint, so the window never flashes the wrong theme.
applyPreferences(readPreferences());

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
      staleTime: Number.POSITIVE_INFINITY,
      refetchOnWindowFocus: false,
    },
  },
});

const root = document.getElementById("root");
if (!root) throw new Error("Application root was not found");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <PresentationProvider>
        <App />
      </PresentationProvider>
    </QueryClientProvider>
  </StrictMode>,
);
