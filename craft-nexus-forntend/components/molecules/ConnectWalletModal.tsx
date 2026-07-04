"use client";

import { FaTimes, FaCheckCircle, FaSignOutAlt } from "react-icons/fa";
import { useState, useEffect } from "react";
import { 
  connectFreighterWallet, 
  isFreighterInstalled,
  storeWallet,
} from "@/lib/stellar/wallet";

const stellarWallets = [
  { 
    id: "freighter", 
    name: "Freighter", 
    description: "Stellar wallet browser extension",
    recommended: true 
  },
  { 
    id: "xbull", 
    name: "XBull", 
    description: "Stellar wallet extension (Coming soon)",
    disabled: true 
  },
  { 
    id: "lobster", 
    name: "Lobster", 
    description: "Stellar wallet extension (Coming soon)",
    disabled: true 
  },
];

interface ConnectWalletModalProps {
  isOpen: boolean;
  handleClose: () => void;
  onConnected?: (publicKey: string) => void;
  connectedAddress?: string | null;
  onDisconnect?: () => void;
}

export const ConnectWalletModal = ({
  isOpen,
  handleClose,
  onConnected,
  connectedAddress,
  onDisconnect,
}: ConnectWalletModalProps) => {
  const [isConnecting, setIsConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [freighterAvailable, setFreighterAvailable] = useState(false);

  useEffect(() => {
    // Check if Freighter is installed
    const checkFreighter = async () => {
      try {
        // Give extension time to load (especially after installation)
        await new Promise(resolve => setTimeout(resolve, 100));
        
        const installed = await isFreighterInstalled();
        console.log("Freighter available:", installed);
        setFreighterAvailable(installed);
        
        // If not available, check again after a delay (extension might be loading)
        if (!installed) {
          setTimeout(async () => {
            const retryCheck = await isFreighterInstalled();
            console.log("Freighter retry check:", retryCheck);
            setFreighterAvailable(retryCheck);
          }, 1000);
        }
      } catch (error) {
        console.error("Error checking Freighter:", error);
        setFreighterAvailable(false);
      }
    };
    
    if (isOpen) {
      checkFreighter();
    }
  }, [isOpen]);

  const handleConnect = async (walletId: string) => {
    if (walletId !== "freighter") {
      setError(`${walletId} wallet integration coming soon`);
      return;
    }

    setIsConnecting(true);
    setError(null);

    try {
      const account = await connectFreighterWallet();
      
      if (account && account.publicKey) {
        storeWallet(account);
        onConnected?.(account.publicKey);
        handleClose();
      } else {
        setError("Failed to connect wallet. Please make sure Freighter is installed and unlocked.");
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Unknown error occurred";
      setError(errorMessage);
    } finally {
      setIsConnecting(false);
    }
  };

  const shortenAddress = (address: string) => {
    if (!address) return "";
    return `${address.slice(0, 8)}...${address.slice(-8)}`;
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-[rgba(0,0,0,.5)] backdrop-blur-[4px] bg-opacity-50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl p-6 w-full max-w-md shadow-2xl animate-in zoom-in-95 duration-200">
        <div className="flex justify-between items-center mb-6">
          <div>
            <h2 className="text-xl font-bold text-gray-900 font-outfit">Connect Stellar Wallet</h2>
            <p className="text-sm text-gray-500 mt-1">
              Connect your wallet to interact with CraftNexus
            </p>
          </div>
          <button
            onClick={handleClose}
            className="text-gray-400 hover:text-gray-600 transition-colors p-1"
            disabled={isConnecting}
          >
            <FaTimes className="text-xl" />
          </button>
        </div>

        {connectedAddress && (
          <div className="mb-6 p-4 bg-green-50 border border-green-100 rounded-xl relative overflow-hidden">
            <div className="flex items-center gap-3 relative z-10">
              <FaCheckCircle className="text-green-500 text-xl flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-xs font-semibold text-green-700 uppercase tracking-wider">Currently Connected</p>
                <p className="text-sm font-mono text-green-900 truncate mt-0.5">{shortenAddress(connectedAddress)}</p>
              </div>
              <button
                onClick={() => {
                  onDisconnect?.();
                  handleClose();
                }}
                className="p-2 text-red-500 hover:bg-red-50 rounded-lg transition-colors flex items-center gap-2 text-sm font-medium"
                title="Disconnect Wallet"
              >
                <FaSignOutAlt />
                <span className="hidden sm:inline">Disconnect</span>
              </button>
            </div>
            <div className="absolute -right-4 -bottom-4 text-green-100 opacity-50 pointer-events-none">
              <FaCheckCircle size={80} />
            </div>
          </div>
        )}

        {error && (
          <div className="mb-6 p-4 bg-red-50 border border-red-100 rounded-xl flex items-start gap-3">
            <div className="text-xs text-red-600 font-medium whitespace-pre-line">{error}</div>
          </div>
        )}

        <div className="space-y-3">
          {stellarWallets.map((wallet) => {
            const isAvailable = wallet.id === "freighter" && freighterAvailable;
            const isDisabled = wallet.disabled || (wallet.id !== "freighter" && !isAvailable) || isConnecting;

            return (
              <button
                key={wallet.id}
                className={`flex items-center gap-4 w-full p-4 border rounded-xl transition-all ${
                  isDisabled
                    ? "border-gray-100 bg-gray-50 cursor-not-allowed opacity-60"
                    : "border-gray-200 hover:bg-gray-50 hover:border-blue-400 hover:shadow-md group"
                }`}
                onClick={() => !isDisabled && handleConnect(wallet.id)}
                disabled={isDisabled}
              >
                <div className="flex-1 text-left">
                  <div className="flex items-center gap-2">
                    <span className="font-bold text-gray-900">{wallet.name}</span>
                    {wallet.recommended && (
                      <span className="text-[10px] font-bold bg-blue-600 text-white px-2 py-0.5 rounded-full uppercase tracking-tighter">
                        Recommended
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-gray-500 mt-1">{wallet.description}</p>
                  {wallet.id === "freighter" && !freighterAvailable && (
                    <p className="text-[10px] text-orange-600 mt-1 font-medium">
                      Extension not detected in browser
                    </p>
                  )}
                </div>
                {isConnecting && wallet.id === "freighter" ? (
                  <div className="animate-spin rounded-full h-5 w-5 border-2 border-blue-600 border-t-transparent"></div>
                ) : (
                  <div className="w-2 h-2 rounded-full bg-gray-200 group-hover:bg-blue-400 transition-colors"></div>
                )}
              </button>
            );
          })}
        </div>

        {!freighterAvailable && !connectedAddress && (
          <div className="mt-6 p-4 bg-blue-50 border border-blue-100 rounded-xl">
            <div className="flex flex-col gap-2 text-xs text-blue-800">
              <p className="font-bold">Freighter not detected</p>
              <p>If you just installed Freighter, please refresh this page and make sure it's enabled.</p>
              <a
                href="https://freighter.app"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center text-blue-600 hover:underline font-bold mt-1"
              >
                Install Freighter Wallet →
              </a>
            </div>
          </div>
        )}

        <p className="text-[10px] text-gray-400 mt-6 text-center leading-relaxed">
          By connecting a wallet, you agree to our <span className="underline cursor-pointer">Terms</span> and <span className="underline cursor-pointer">Privacy Policy</span>
        </p>
      </div>
    </div>
  );
};
