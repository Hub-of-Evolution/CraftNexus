"use client";

import { FaTimes } from "react-icons/fa";
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
}

export const ConnectWalletModal = ({
  isOpen,
  handleClose,
  onConnected,
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

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-[rgba(0,0,0,.5)] backdrop-blur-[4px] bg-opacity-50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl p-6 w-full max-w-md">
        <div className="flex justify-between items-center mb-4">
          <div>
            <h2 className="text-xl font-bold">Connect Stellar Wallet</h2>
            <p className="text-sm text-gray-500 mt-1">
              Connect your Stellar wallet to buy and sell on CraftNexus
            </p>
          </div>
          <button
            onClick={handleClose}
            className="text-gray-500 hover:text-gray-700 cursor-pointer"
            disabled={isConnecting}
          >
            <FaTimes className="text-xl" />
          </button>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-lg">
            <p className="text-sm text-red-600 whitespace-pre-line">{error}</p>
          </div>
        )}

        <div className="space-y-3">
          {stellarWallets.map((wallet) => {
            // Allow Freighter connection even if initial check failed (user can try)
            const isAvailable = wallet.id === "freighter" && freighterAvailable;
            const isDisabled = wallet.disabled || (wallet.id !== "freighter" && !isAvailable) || isConnecting;

            return (
              <button
                key={wallet.id}
                className={`flex items-start gap-4 w-full p-4 border rounded-lg transition-colors ${
                  isDisabled
                    ? "border-gray-200 bg-gray-50 cursor-not-allowed opacity-50"
                    : "border-gray-200 hover:bg-gray-50 hover:border-blue-300"
                }`}
                onClick={() => !isDisabled && handleConnect(wallet.id)}
                disabled={isDisabled}
              >
                <div className="flex-1 text-left">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{wallet.name}</span>
                    {wallet.recommended && (
                      <span className="text-xs bg-blue-100 text-blue-600 px-2 py-0.5 rounded">
                        Recommended
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-gray-500 mt-1">{wallet.description}</p>
                  {wallet.id === "freighter" && !freighterAvailable && (
                    <p className="text-xs text-orange-600 mt-1">
                      Freighter may not be detected. You can still try connecting.
                    </p>
                  )}
                </div>
                {isConnecting && wallet.id === "freighter" && (
                  <div className="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-600"></div>
                )}
              </button>
            );
          })}
        </div>

        {!freighterAvailable && (
          <div className="mt-4 p-3 bg-blue-50 border border-blue-200 rounded-lg">
            <p className="text-sm text-blue-800 mb-2">
              <strong>Freighter not detected</strong>
            </p>
            <p className="text-xs text-blue-700 mb-2">
              If you just installed Freighter, please:
            </p>
            <ol className="text-xs text-blue-700 list-decimal list-inside mb-2 space-y-1">
              <li>Refresh this page</li>
              <li>Make sure the extension is enabled</li>
              <li>Try connecting again</li>
            </ol>
            <a
              href="https://freighter.app"
              target="_blank"
              rel="noopener noreferrer"
              className="text-sm text-blue-600 hover:underline font-medium"
            >
              Install Freighter Wallet →
            </a>
          </div>
        )}

        <p className="text-xs text-gray-500 mt-4 text-center">
          By connecting a wallet, you agree to our Terms of Service and Privacy Policy
        </p>
      </div>
    </div>
  );
};
