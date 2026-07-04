"use client";
import { FaBars, FaTimes, FaSearch, FaSignOutAlt } from "react-icons/fa";
import Image from "next/image";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState, useEffect } from "react";
import { ConnectWalletModal } from "./ConnectWalletModal";
import { getStoredWallet, disconnectWallet } from "@/lib/stellar/wallet";

export const MobileNav = () => {
  const [isWalletModalOpen, setIsWalletModalOpen] = useState(false);
  const [publicKey, setPublicKey] = useState<string | null>(null);
  const [isOpen, setIsOpen] = useState(false);
  const pathname = usePathname();

  useEffect(() => {
    const storedWallet = getStoredWallet();
    if (storedWallet) {
      setPublicKey(storedWallet.publicKey);
    }
  }, []);

  const handleDisconnect = () => {
    disconnectWallet();
    setPublicKey(null);
    closeMenu();
  };

  const toggleMenu = () => {
    setIsOpen(!isOpen);
  };

  const closeMenu = () => {
    setIsOpen(false);
  };

  const shortenAddress = (address: string) => {
    if (!address) return "";
    return `${address.slice(0, 6)}...${address.slice(-4)}`;
  };

  const isActive = (path: string) => pathname === path;

  return (
    <div className="md:hidden flex flex-col w-full shadow-sm">
      <ConnectWalletModal
        isOpen={isWalletModalOpen}
        handleClose={() => setIsWalletModalOpen(false)}
        onConnected={(key) => setPublicKey(key)}
        connectedAddress={publicKey}
        onDisconnect={handleDisconnect}
      />
      <div className="flex justify-between items-center p-4 px-4">
        <Link href="/">
          <Image src="/logo.svg" alt="CraftNexus" width={80} height={80} />
        </Link>
        <button onClick={toggleMenu} className="text-2xl">
          {isOpen ? (
            <FaTimes className="text-gray-800 cursor-pointer" />
          ) : (
            <FaBars className="text-gray-800 cursor-pointer" />
          )}
        </button>
      </div>
      {isOpen && (
        <div className="flex flex-col items-center gap-4 bg-white py-4 border-b">
          <div className="border-1 border-gray-300 rounded-xl py-2 px-4 gap-4 relative flex items-center w-[90%]">
            <FaSearch className="text-gray-500 text-xl" />
            <input
              type="text"
              placeholder="Search for courses..."
              className="border-0 outline-0 w-full"
            />
          </div>
          <Link
            href="/market"
            className={`text-black font-inter text-sm ${
              isActive("/market") ? "font-bold" : ""
            }`}
            onClick={closeMenu}
          >
            Market
          </Link>
          <Link
            href="/workshop"
            className={`text-black font-inter text-sm ${
              isActive("/workshop") ? "font-bold" : ""
            }`}
            onClick={closeMenu}
          >
            Workshop
          </Link>
          <Link
            href="/contact"
            className={`text-black font-inter text-sm ${
              isActive("/contact") ? "font-bold" : ""
            }`}
            onClick={closeMenu}
          >
            Contact
          </Link>
          
          {publicKey ? (
            <div className="flex flex-col items-center gap-3 w-full px-4">
              <div className="flex items-center gap-2 p-2 px-4 bg-gray-50 rounded-lg w-full justify-center">
                <span className="font-mono text-sm">{shortenAddress(publicKey)}</span>
              </div>
              <button
                onClick={handleDisconnect}
                className="flex items-center gap-2 text-red-600 font-medium py-2 px-4 border rounded-lg w-full justify-center hover:bg-red-50"
              >
                <FaSignOutAlt size={16} />
                Disconnect
              </button>
            </div>
          ) : (
            <button
              onClick={() => {
                setIsWalletModalOpen(true);
                setIsOpen(false);
              }}
              className="border-1 py-2 px-4 rounded-lg shadow-sm cursor-pointer hover:bg-gray-50 w-[90%]"
            >
              Connect Wallet
            </button>
          )}
        </div>
      )}
    </div>
  );
};
