import type { Metadata } from "next";
import { Geist, Geist_Mono, Inter, Outfit } from "next/font/google";

import "./globals.css";

import { IBM_Plex_Serif, Poppins } from "next/font/google";
import RouteWrapper from "@/components/layout/RouteWrapper";

const ibmPlexSerif = IBM_Plex_Serif({
  weight: ["400", "600", "700"],
  subsets: ["latin"],
  variable: "--font-ibm-plex-serif",
});

const poppins = Poppins({
  weight: ["400", "500", "600", "700"], // Specify required weights
  subsets: ["latin"],
  variable: "--font-poppins",
});

const outfit = Outfit({
  subsets: ["latin"],
  variable: "--font-outfit",
});

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});
const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  weight: ["100", "200", "300", "400", "500", "600", "700", "800", "900"],
  display: "swap",
});

export const metadata: Metadata = {
  title: "CraftNexus - Handcraft Marketplace",
  description: "Sell and learn handcraft. Connect with artisans, discover unique handmade products, and grow your creative business.",
  icons: {
    icon: "/logo.svg",
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body
        className={`${inter.variable} ${geistSans.variable} ${geistMono.variable} ${ibmPlexSerif.variable} ${poppins.variable} ${outfit.variable} antialiased`}
      >
        <RouteWrapper>{children}</RouteWrapper>
      </body>
    </html>
  );
}
