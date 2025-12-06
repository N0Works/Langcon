import React from "react";
import ReactDOM from "react-dom/client";
import { ThemeProvider } from "next-themes";
import { LanguageProvider } from "@/components/language-provider";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <ThemeProvider attribute="class" defaultTheme="light" enableSystem={false}>
        <App />
      </ThemeProvider>
    </LanguageProvider>
  </React.StrictMode>,
);
