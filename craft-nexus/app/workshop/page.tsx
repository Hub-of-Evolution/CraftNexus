"use client";

import React from "react";
import { 
  LuTrendingUp, 
    LuLoaderCircle, 
  LuCircleCheck, 
  LuBox, 
  LuArrowRight 
} from "react-icons/lu";
import Link from "next/link";

interface StatItemProps {
  label: string;
  value: string | number;
  icon: React.ReactNode;
  trend?: string;
  trendUp?: boolean;
}

const StatCard = ({ label, value, icon, trend, trendUp }: StatItemProps) => (
  <div className="bg-white p-6 rounded-2xl border border-gray-100 shadow-sm hover:shadow-md transition-shadow duration-300 group">
    <div className="flex justify-between items-start mb-4">
      <div className="p-3 bg-gray-50 rounded-xl group-hover:bg-[#C4928F]/10 text-gray-400 group-hover:text-[#C4928F] transition-colors duration-300">
        {icon}
      </div>
      {trend && (
        <span className={`text-[10px] font-bold px-2 py-1 rounded-full ${trendUp ? "bg-green-50 text-green-600" : "bg-red-50 text-red-600"}`}>
          {trendUp ? "↑" : "↓"} {trend}
        </span>
      )}
    </div>
    <p className="text-xs font-semibold text-gray-500 uppercase tracking-tight mb-1">{label}</p>
    <h3 className="text-2xl font-bold text-gray-900 font-outfit">{value}</h3>
  </div>
);

export default function WorkshopOverview() {
  return (
    <div className="space-y-8 pb-10">
      {/* Premium Header */}
      <section className="flex flex-col md:flex-row justify-between items-start md:items-center gap-6">
        <div className="space-y-1">
          <h1 className="text-3xl font-bold text-gray-900 font-serif">Workshop Overview</h1>
          <p className="text-sm text-gray-500">Welcome back, Artisan. Here's what's happening today.</p>
        </div>
        <div className="flex items-center gap-3">
          <div className="flex -space-x-2">
            {[1, 2, 3].map((i) => (
              <div key={i} className="w-8 h-8 rounded-full border-2 border-white bg-gray-200" />
            ))}
          </div>
          <span className="text-xs text-gray-500 font-medium">12 active buyers interested</span>
        </div>
      </section>

      {/* Stats Grid */}
      <section className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
        <StatCard 
          label="Reputation Score" 
          value="98.5%" 
          icon={<LuTrendingUp size={24} />} 
          trend="2.4%" 
          trendUp={true} 
        />
        <StatCard 
          label="Active Escrows" 
          value="12" 
          icon={<LuBox size={24} />} 
        />
        <StatCard 
          label="Successful Trades" 
          value="156" 
          icon={<LuCircleCheck size={24} />} 
          trend="12" 
          trendUp={true} 
        />
        <StatCard 
          label="Disputed (30d)" 
          value="1" 
          icon={<  LuLoaderCircle size={24} />} 
          trend="None" 
          trendUp={true} 
        />
      </section>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Recent Activity / Active Tasks */}
        <section className="lg:col-span-2 space-y-4">
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-lg font-bold text-gray-900 px-1">Critical Actions</h3>
            <Link href="/workshop/history" className="text-xs font-bold text-[#C4928F] hover:underline flex items-center gap-1">
              View All History <LuArrowRight size={14} />
            </Link>
          </div>
          
          <div className="space-y-4">
            <div className="bg-white p-5 rounded-2xl border border-gray-100 flex items-center justify-between group cursor-pointer hover:border-[#C4928F]/30 transition-all duration-300">
              <div className="flex items-center gap-4">
                <div className="w-12 h-12 bg-rose-50 text-rose-500 rounded-xl flex items-center justify-center shrink-0">
                  <LuBox size={24} />
                </div>
                <div>
                  <h4 className="text-sm font-bold text-gray-900 group-hover:text-[#C4928F] transition-colors uppercase tracking-tight">Escrow #425 - Funded</h4>
                  <p className="text-xs text-gray-500">Buyer funded 2,500 USDC. Waiting for your verification.</p>
                </div>
              </div>
              <button className="bg-gray-900 text-white text-[10px] font-bold px-4 py-2 rounded-lg hover:bg-[#C4928F] transition-colors uppercase">
                Manage
              </button>
            </div>

            <div className="bg-white p-5 rounded-2xl border border-gray-100 flex items-center justify-between group cursor-pointer hover:border-[#C4928F]/30 transition-all duration-300">
              <div className="flex items-center gap-4">
                <div className="w-12 h-12 bg-blue-50 text-blue-500 rounded-xl flex items-center justify-center shrink-0">
                  <LuLoaderCircle size={24} />
                </div>
                <div>
                  <h4 className="text-sm font-bold text-gray-900 group-hover:text-[#C4928F] transition-colors uppercase tracking-tight">Staking Cooldown Ending</h4>
                  <p className="text-xs text-gray-500">Your unstaking window opens in 14 hours.</p>
                </div>
              </div>
              <Link href="/workshop/staking" className="bg-gray-100 text-gray-700 text-[10px] font-bold px-4 py-2 rounded-lg hover:bg-gray-200 transition-colors uppercase">
                Go to Staking
              </Link>
            </div>
          </div>
        </section>

        {/* Security / Reputation Card */}
        <section className="bg-gray-900 rounded-3xl p-8 text-white relative overflow-hidden flex flex-col justify-between">
          <div className="relative z-10">
            <LuLoaderCircle className="text-[#C4928F] text-4xl mb-4" />
            <h3 className="text-xl font-bold mb-2">Verified Artisan</h3>
            <p className="text-sm text-gray-400 leading-relaxed font-sans mb-6">
              You are currently at **Tier 2**. Complete 25 more trades to unlock the **Elite Artisan** badge and 0.5% lower fees.
            </p>
            <div className="h-2 w-full bg-gray-800 rounded-full mb-2">
              <div className="h-full bg-[#C4928F] rounded-full w-[74%]" />
            </div>
            <p className="text-[10px] text-gray-500 text-right uppercase tracking-widest font-bold">Progress: 74%</p>
          </div>
          
          <div className="absolute top-[-20px] right-[-20px] w-40 h-40 bg-[#C4928F] opacity-10 rounded-full blur-3xl pointer-events-none" />
          
          <Link 
            href="/workshop/verification"
            className="w-full bg-[#C4928F] text-white text-xs font-bold py-3 rounded-xl hover:bg-[#B37E7A] transition-all text-center uppercase tracking-widest relative z-10"
          >
            Upgrade Tier
          </Link>
        </section>
      </div>
    </div>
  );
}
