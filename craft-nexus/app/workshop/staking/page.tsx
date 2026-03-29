"use client";

import Link from "next/link";
import React, { useState, useEffect } from "react";
import { 
  LuShieldCheck, 
  LuTimer, 
  LuTrendingUp, 
  LuArrowUpRight, 
  LuArrowDownLeft 
} from "react-icons/lu";

export default function StakingHub() {
  const [amount, setAmount] = useState("");
  const [isStaking, setIsStaking] = useState(true); // Toggle between Stake/Unstake

  return (
    <div className="space-y-8 pb-10">
      {/* Header */}
      <section className="space-y-1">
        <h1 className="text-3xl font-bold text-gray-900 font-serif lowercase tracking-tight">Staking Hub</h1>
        <p className="text-sm text-gray-500 font-sans">Secure your reputation by staking assets. This provides trust to buyers and unlocks lower platform fees.</p>
      </section>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Staking Controls - Left Side (2 cols on large) */}
        <div className="lg:col-span-2 space-y-8">
          
          {/* Main Interaction Card */}
          <div className="bg-white rounded-3xl border border-gray-100 shadow-xl overflow-hidden relative">
            <div className="bg-gray-50 p-6 flex items-center justify-between border-b border-gray-200">
              <div className="flex bg-gray-200/50 p-1 rounded-xl">
                <button 
                  onClick={() => setIsStaking(true)}
                  className={`px-6 py-2 rounded-lg text-xs font-bold transition-all ${isStaking ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-900 uppercase tracking-widest"}`}
                >
                  STAKE
                </button>
                <button 
                  onClick={() => setIsStaking(false)}
                  className={`px-6 py-2 rounded-lg text-xs font-bold transition-all ${!isStaking ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-900 uppercase tracking-widest"}`}
                >
                  UNSTAKE
                </button>
              </div>
              <div className="text-right">
                <p className="text-[10px] text-gray-400 font-bold uppercase tracking-widest">Available Balance</p>
                <p className="text-sm font-bold text-gray-900">12,450.00 USDC</p>
              </div>
            </div>

            <div className="p-10 space-y-8">
              <div className="space-y-4">
                <label className="text-xs font-bold text-gray-400 uppercase tracking-widest block px-1">Amount to {isStaking ? "Stake" : "Unstake"}</label>
                <div className="relative group">
                  <input 
                    type="number" 
                    value={amount}
                    onChange={(e) => setAmount(e.target.value)}
                    placeholder="0.00"
                    className="w-full text-5xl font-outfit font-bold text-gray-900 bg-transparent border-0 outline-0 placeholder:text-gray-100 pb-4 border-b-2 border-gray-50 focus:border-[#C4928F] transition-colors duration-300"
                  />
                  <span className="absolute right-0 bottom-6 text-2xl font-bold text-gray-400 group-focus-within:text-[#C4928F] transition-colors">USDC</span>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-6">
                <div className="p-4 bg-gray-50 rounded-2xl border border-gray-100 flex items-center gap-4">
                  <div className="p-2 bg-white rounded-lg text-[#C4928F] shadow-sm">
                    <LuTrendingUp size={18} />
                  </div>
                  <div>
                    <p className="text-[10px] text-gray-400 font-bold uppercase tracking-tight">Est. Tier Increase</p>
                    <p className="text-sm font-bold text-gray-900">+12% Progress</p>
                  </div>
                </div>
                {!isStaking && (
                  <div className="p-4 bg-rose-50 rounded-2xl border border-rose-100 flex items-center gap-4 animate-in slide-in-from-right-2">
                    <div className="p-2 bg-white rounded-lg text-rose-500 shadow-sm">
                      <LuTimer size={18} />
                    </div>
                    <div>
                      <p className="text-[10px] text-rose-400 font-bold uppercase tracking-tight">Cooldown Penalty</p>
                      <p className="text-sm font-bold text-rose-600">7 Days Lock</p>
                    </div>
                  </div>
                )}
              </div>

              <button className={`w-full py-5 rounded-2xl text-sm font-bold uppercase tracking-widest shadow-lg transition-all transform hover:translate-y-[-2px] active:translate-y-[0px] ${
                isStaking 
                  ? "bg-gray-900 text-white hover:bg-[#C4928F] shadow-gray-200" 
                  : "bg-white text-rose-600 border border-rose-100 hover:bg-rose-50 shadow-rose-100"
              }`}>
                Confirm {isStaking ? "Stake" : "Unstake"}
              </button>
            </div>
            
            {/* Visual background accents */}
            <div className="absolute top-[-10%] right-[-10%] w-[40%] h-[40%] bg-[#C4928F] opacity-[0.03] rounded-full blur-3xl pointer-events-none" />
          </div>

          {/* Staking History (Small List) */}
          <div className="space-y-4 px-2">
            <h3 className="text-sm font-bold text-gray-900 uppercase tracking-widest font-inter">Recent Staking Events</h3>
            <div className="space-y-3">
              <div className="flex items-center justify-between p-4 bg-white rounded-xl border border-gray-100 text-sm font-sans">
                <div className="flex items-center gap-3">
                  <div className="p-2 bg-green-50 text-green-500 rounded-lg">
                    <LuArrowUpRight size={14} />
                  </div>
                  <div>
                    <p className="font-bold text-gray-900 tracking-tight">Asset Staked</p>
                    <p className="text-[10px] text-gray-500 uppercase">March 28, 2026</p>
                  </div>
                </div>
                <p className="font-bold text-gray-900 italic">+500.00 USDC</p>
              </div>
              <div className="flex items-center justify-between p-4 bg-white rounded-xl border border-gray-100 text-sm font-sans">
                <div className="flex items-center gap-3">
                  <div className="p-2 bg-gray-50 text-gray-400 rounded-lg">
                    <LuArrowDownLeft size={14} />
                  </div>
                  <div>
                    <p className="font-bold text-gray-900 tracking-tight">Unstaking Initiated</p>
                    <p className="text-[10px] text-gray-500 uppercase">March 15, 2026</p>
                  </div>
                </div>
                <p className="font-bold text-gray-400 italic font-mono uppercase tracking-tighter">-200.00 USDC</p>
              </div>
            </div>
          </div>
        </div>

        {/* Sidebar - Right Side (1 col) */}
        <div className="space-y-8">
          
          {/* Active Stake Summary */}
          <section className="bg-gray-900 rounded-3xl p-8 text-white flex flex-col justify-between min-h-[400px]">
            <div>
              <div className="w-12 h-12 bg-white/5 rounded-2xl flex items-center justify-center mb-8 border border-white/5 group-hover:border-[#C4928F]/30 transition-colors duration-300">
                <LuShieldCheck className="text-[#C4928F]" size={24} />
              </div>
              
              <p className="text-xs font-bold text-gray-500 uppercase tracking-widest mb-2 font-inter">Total Staked Value</p>
              <h2 className="text-4xl font-bold font-outfit mb-2">2,500.00 <span className="text-base text-gray-500 font-medium">USDC</span></h2>
              <p className="text-xs text-green-400 font-bold mb-8 uppercase tracking-tight">↑ +24.5% vs Last Period</p>

              <div className="space-y-6 pt-6 border-t border-white/5">
                <div className="flex justify-between items-end">
                  <div className="space-y-1">
                    <p className="text-[10px] text-gray-500 font-bold uppercase tracking-widest">Active Cooldown</p>
                    <p className="text-sm font-bold flex items-center gap-2">
                       72 Hours Left <LuTimer className="text-[#C4928F]" size={14} />
                    </p>
                  </div>
                  <div className="text-[10px] font-bold text-gray-500 uppercase tracking-tight italic">Tier 2 Protection</div>
                </div>

                <div className="h-2 w-full bg-white/5 rounded-full overflow-hidden">
                  <div className="h-full bg-gradient-to-r from-[#C4928F]/80 to-[#C4928F] w-[34%]" />
                </div>
              </div>
            </div>
            
            <Link 
              href="/workshop/verification"
              className="mt-10 py-4 bg-white/5 hover:bg-white/10 rounded-2xl text-[10px] font-bold uppercase tracking-widest text-[#C4928F] transition-all border border-white/5 text-center"
            >
              Learn about staking rules
            </Link>
          </section>

          {/* Asset Allocation Tip */}
          <div className="p-6 bg-amber-50 rounded-2xl border border-amber-100 relative group overflow-hidden">
            <h4 className="text-xs font-bold text-amber-800 uppercase tracking-widest mb-1 font-serif">Staking Tip</h4>
            <p className="text-xs text-amber-700 leading-relaxed font-sans">
              Artisans with at least **500 XLM** staked receive a 15% discount on username change fees.
            </p>
            <LuShieldCheck className="absolute bottom-[-10px] right-[-10px] text-amber-800 opacity-[0.05] rotate-12 transition-transform duration-500 group-hover:scale-110" size={100} />
          </div>

        </div>
      </div>
    </div>
  );
}
