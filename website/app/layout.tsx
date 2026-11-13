import type { Metadata, Viewport } from "next";
import localFont from "next/font/local";
import type { ReactNode } from "react";
import { MotionProvider } from "@/components/motion/motion-provider";
import { Footer } from "@/components/sections/footer";
import { Header } from "@/components/sections/header";
import { DESCRIPTION, SITE, TITLE } from "@/lib/site";
import "./globals.css";

const instrument = localFont({
  variable: "--font-instrument",
  display: "swap",
  src: [
    { path: "../public/fonts/instrument-sans-latin-400-normal.woff2", weight: "400" },
    { path: "../public/fonts/instrument-sans-latin-500-normal.woff2", weight: "500" },
    { path: "../public/fonts/instrument-sans-latin-600-normal.woff2", weight: "600" },
  ],
});
const plex = localFont({
  variable: "--font-plex",
  display: "swap",
  preload: false,
  src: [
    { path: "../public/fonts/ibm-plex-mono-latin-400-normal.woff2", weight: "400" },
    { path: "../public/fonts/ibm-plex-mono-latin-500-normal.woff2", weight: "500" },
  ],
});

export const metadata: Metadata = {
  metadataBase: new URL(SITE),
  title: TITLE,
  description: DESCRIPTION,
  robots: { index: true, follow: true, "max-image-preview": "large" },
  icons: { icon: { url: "/favicon.svg", type: "image/svg+xml" }, apple: "/apple-touch-icon.png" },
};

export const viewport: Viewport = { colorScheme: "dark light", themeColor: "#0c1a2b" };

// Runs before paint: saved theme (dark by default), JS flag, motion flag, screenshot source swap.
const HEAD = `(()=>{var r=document.documentElement,t="dark";try{if(localStorage.getItem("tableiq-theme")==="light")t="light"}catch(e){}r.dataset.theme=t;r.classList.add("js");if(!matchMedia("(prefers-reduced-motion: reduce)").matches)r.classList.add("motion");document.addEventListener("DOMContentLoaded",function(){if(t==="light")document.querySelectorAll("source[data-theme-source='dark']").forEach(function(s){s.media="not all"})})})()`;

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" data-theme="dark" className={`${instrument.variable} ${plex.variable}`} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: HEAD }} />
        <link rel="describedby" href="/llms.txt" type="text/plain" />
      </head>
      <body>
        <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:z-[100] focus:rounded-pill focus:bg-ink focus:px-4 focus:py-2 focus:text-paper">Skip to content</a>
        <MotionProvider>
          <Header />
          <main id="main">{children}</main>
          <Footer />
        </MotionProvider>
      </body>
    </html>
  );
}
