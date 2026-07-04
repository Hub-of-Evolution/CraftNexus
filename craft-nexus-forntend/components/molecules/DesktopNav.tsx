"use client";
import { FaSearch, FaChevronDown, FaSignOutAlt } from "react-icons/fa";
import Image from "next/image";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState, useEffect } from "react";
import { ConnectWalletModal } from "./ConnectWalletModal";
import { getStoredWallet, disconnectWallet } from "@/lib/stellar/wallet";

export const DesktopNav = () => {
  const [isWalletModalOpen, setIsWalletModalOpen] = useState(false);
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const pathname = usePathname();

  const isActive = (path: string) => pathname === path;

  // Initialize wallet state from localStorage
  useEffect(() => {
    const storedWallet = getStoredWallet();
    if (storedWallet) {
      setPublicKey(storedWallet.publicKey);
    }
  }, []);

  const handleDisconnect = () => {
    disconnectWallet();
    setPublicKey(null);
    setIsDropdownOpen(false);
  };

  const shortenAddress = (address: string) => {
    if (!address) return "";
    return `${address.slice(0, 4)}...${address.slice(-4)}`;
  };

  return (
    <div className="hidden md:flex flex-row justify-between items-center w-[90vw] mx-auto py-4">
      <ConnectWalletModal
        isOpen={isWalletModalOpen}
        handleClose={() => setIsWalletModalOpen(false)}
        onConnected={(key) => setPublicKey(key)}
        connectedAddress={publicKey}
        onDisconnect={handleDisconnect}
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
          className={`${isActive("/market") ? "font-bold text-primary" : ""}`}
        >
          Market
        </Link>
        <Link
          href="/workshop"
          className={`${isActive("/workshop") ? "font-bold text-primary" : ""}`}
        >
          Workshop
        </Link>
        <Link
          href="/contact"
          className={`${isActive("/contact") ? "font-bold text-primary" : ""}`}
        >
          Contact
        </Link>

        {publicKey ? (
          <div className="relative">
            <button
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              className="flex items-center gap-2 border-1 py-2 px-2 lg:px-4 rounded-lg shadow-sm cursor-pointer hover:bg-gray-50 transition-all font-mono"
            >
              {shortenAddress(publicKey)}
              <FaChevronDown className={`text-xs transition-transform ${isDropdownOpen ? "rotate-180" : ""}`} />
            </button>

            {isDropdownOpen && (
              <div className="absolute right-0 mt-2 w-48 bg-white border border-gray-100 rounded-lg shadow-lg z-[60] py-1 animate-in fade-in slide-in-from-top-1">
                <div className="px-4 py-2 border-b border-gray-50">
                  <p className="text-xs text-gray-500 truncate">{publicKey}</p>
                </div>
                <button
                  onClick={handleDisconnect}
                  className="flex items-center gap-2 w-full text-left px-4 py-2 text-sm text-red-600 hover:bg-red-50 transition-colors"
                >
                  <FaSignOutAlt size={14} />
                  Disconnect
                </button>
              </div>
            )}
          </div>
        ) : (
          <button
            onClick={() => setIsWalletModalOpen(true)}
            className="border-1 py-2 px-2 lg:px-4 rounded-lg shadow-sm cursor-pointer hover:translate-y-[-3px] focus:translate-y-[-3px] transition-all"
          >
            Connect Wallet
          </button>
        )}
      </div>
    </div>
  );
};
