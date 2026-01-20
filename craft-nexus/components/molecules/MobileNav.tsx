"use client";
import { FaBars, FaTimes, FaSearch } from "react-icons/fa";
import Image from "next/image";
import Link from "next/link";
import { useState } from "react";
import { ConnectWalletModal } from "./ConnectWalletModal";
export const MobileNav = () => {
  const [isWalletModalOpen, setIsWalletModalOpen] = useState(false);
  const [isOpen, setIsOpen] = useState(false);

  const toggleMenu = () => {
    setIsOpen(!isOpen);
  };

  return (
    <div className="md:hidden flex flex-col w-full  shadow-sm">
      <ConnectWalletModal
        isOpen={isWalletModalOpen}
        handleClose={() => setIsWalletModalOpen(false)}
      />
      <div className="flex justify-between items-center  p-4 px-4">
        <Link href="">
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
        <div className="flex flex-col items-center gap-4 bg-white py-4">
          <div className="border-1 border-gray-300 rounded-xl py-2 px-4 gap-4 relative flex items-center w-[90%]">
            <FaSearch className="text-gray-500 text-xl" />
            <input
              type="text"
              placeholder="Search for courses..."
              className="border-0 outline-0 w-full"
            />
          </div>
          <Link href="/market" className="text-black font-inter text-sm">
            Market
          </Link>
          <Link href="/workshop" className="text-black font-inter text-sm">
            Workshop
          </Link>
          <Link href="/contact" className="text-black font-inter text-sm">
            Contact
          </Link>
          <button
            onClick={() => setIsWalletModalOpen(true)}
            className="border-1 py-2 px-4 rounded-lg shadow-sm cursor-pointer hover:translate-y-[-3px] focus:translate-y-[-3px] transition-all"
          >
            Connect Wallet
          </button>
        </div>
      )}
    </div>
  );
};
