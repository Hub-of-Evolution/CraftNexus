"use client";

import React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { 
  LuLayoutDashboard, 
  LuShieldCheck, 
  LuBadgeCheck, 
  LuHistory,
  LuSettings 
} from "react-icons/lu";

interface NavItemProps {
  href: string;
  icon: React.ReactNode;
  label: string;
  active: boolean;
}

const NavItem = ({ href, icon, label, active }: NavItemProps) => (
  <Link
    href={href}
    className={`flex items-center gap-3 px-4 py-3 rounded-xl transition-all duration-200 group ${
      active 
        ? "bg-[#C4928F] text-white shadow-lg shadow-[#C4928F]/20" 
        : "text-gray-500 hover:bg-gray-100"
    }`}
  >
    <span className={`text-xl ${active ? "text-white" : "group-hover:text-[#C4928F]"}`}>
      {icon}
    </span>
    <span className="font-medium text-sm">{label}</span>
  </Link>
);

export default function WorkshopLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const pathname = usePathname();

  const navItems = [
    { href: "/workshop", icon: <LuLayoutDashboard />, label: "Overview" },
    { href: "/workshop/staking", icon: <LuShieldCheck />, label: "Staking Hub" },
    { href: "/workshop/verification", icon: <LuBadgeCheck />, label: "Verification" },
    { href: "/workshop/history", icon: <LuHistory />, label: "History" },
    { href: "/workshop/settings", icon: <LuSettings />, label: "Settings" },
  ];

  return (
    <div className="flex min-h-screen bg-[#FDFBFB] pt-2">
      {/* Sidebar */}
      <aside className="w-64 border-r border-gray-100 p-6 hidden lg:block sticky top-20 h-[calc(100vh-80px)]">
        <div className="space-y-2">
          <p className="text-[10px] font-bold text-gray-400 uppercase tracking-widest px-4 mb-4">
            Artisan Workshop
          </p>
          <nav className="space-y-1">
            {navItems.map((item) => (
              <NavItem
                key={item.href}
                href={item.href}
                icon={item.icon}
                label={item.label}
                active={pathname === item.href}
              />
            ))}
          </nav>
        </div>

        {/* Support Section */}
        <div className="absolute bottom-10 left-6 right-6 p-4 bg-gray-50 rounded-2xl border border-gray-100">
          <p className="text-xs font-semibold text-gray-700 mb-1">Need help?</p>
          <p className="text-[10px] text-gray-500 mb-3">Check our artisan guides or contact support.</p>
          <Link 
            href="/contact" 
            className="text-[10px] font-bold text-[#C4928F] hover:underline"
          >
            Go to Support
          </Link>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 p-4 lg:p-8 overflow-y-auto">
        <div className="max-w-6xl mx-auto animate-in fade-in duration-500">
          {children}
        </div>
      </main>
    </div>
  );
}
