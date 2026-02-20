"use client";
import { FaSearch } from "react-icons/fa";
import Image from "next/image";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState } from "react";
import { ConnectWalletModal } from "./ConnectWalletModal";

export const DesktopNav = () => {
  const [isWalletModalOpen, setIsWalletModalOpen] = useState(false);
  const pathname = usePathname();

  const isActive = (path: string) => pathname === path;

  return (
    <div className="hidden md:flex flex-row justify-between items-center w-[90vw] mx-auto py-4">
      <ConnectWalletModal
        isOpen={isWalletModalOpen}
        handleClose={() => setIsWalletModalOpen(false)}
      />
      <div className="flex flex-row items-center">
        <Link href="/">
          <Image src="/logo.svg" alt="CraftNexus" width={100} height={100} />
        </Link>
      </div>
      <div className="border-1 border-gray-300 rounded-xl py-2 px-4  gap-4 relative flex items-center w-[200px] lg:w-[350px]">
        <FaSearch className="text-gray-500 text-xl" />
        <input
          type="text"
          placeholder="Search for courses..."
          className="border-0 outline-0 w-full"
        />
      </div>
      <div className="flex flex-row items-center text-black gap-4 lg:gap-8 font-inter text-sm lg:text-base">
        <Link
          href="/market"
          className={`${isActive("/market") ? "font-bold" : ""}`}
        >
          Market
        </Link>
        <Link
          href="/workshop"
          className={`${isActive("/workshop") ? "font-bold" : ""}`}
        >
          Workshop
        </Link>
        <Link
          href="/contact"
          className={`${isActive("/contact") ? "font-bold" : ""}`}
        >
          Contact
        </Link>
        <button
          onClick={() => setIsWalletModalOpen(true)}
          className="border-1 py-2 px-2  lg:px-4 rounded-lg shadow-sm cursor-pointer hover:translate-y-[-3px] focus:translate-y-[-3px] transition-all"
        >
          Connect Wallet
        </button>
      </div>
    </div>
  );
};
