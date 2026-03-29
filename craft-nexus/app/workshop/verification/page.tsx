"use client";

import React from "react";
import { 
  LuBadgeCheck, 
  LuZap, 
  LuUserCheck, 
  LuShieldCheck, 
  LuTrendingUp, 
  LuCircleCheck, 
  LuHistory 
} from "react-icons/lu";

interface MetricCardProps {
  label: string;
  current: string | number;
  target: string | number;
  progress: number;
  icon: React.ReactNode;
}

const MetricCard = ({ label, current, target, progress, icon }: MetricCardProps) => (
  <div className="bg-white p-8 rounded-3xl border border-gray-100 shadow-sm transition-all hover:shadow-md group">
    <div className="flex items-center gap-4 mb-6">
      <div className="p-3 bg-gray-50 rounded-2xl group-hover:bg-[#C4928F]/10 text-gray-400 group-hover:text-[#C4928F] transition-colors">
        {icon}
      </div>
      <div>
        <p className="text-[10px] font-bold text-gray-400 uppercase tracking-widest">{label}</p>
        <p className="text-xl font-bold text-gray-900 font-outfit">{current} <span className="text-gray-300">/ {target}</span></p>
      </div>
    </div>
    
    <div className="space-y-2">
      <div className="flex justify-between items-center px-1">
        <span className="text-[10px] font-bold text-gray-500 uppercase">Progress</span>
        <span className="text-[10px] font-bold text-[#C4928F]">{progress}%</span>
      </div>
      <div className="h-3 w-full bg-gray-50 rounded-full overflow-hidden border border-gray-100/50">
        <div 
          className="h-full bg-gradient-to-r from-[#C4928F]/60 to-[#C4928F] rounded-full transition-all duration-1000" 
          style={{ width: `${progress}%` }} 
        />
      </div>
    </div>
  </div>
);

export default function VerificationPortal() {
  return (
    <div className="space-y-10 pb-10">
      {/* Header */}
      <section className="flex flex-col md:flex-row justify-between items-start md:items-center gap-6">
        <div className="space-y-1">
          <div className="flex items-center gap-2 mb-2">
            <span className="bg-rose-50 text-[#C4928F] text-[9px] font-black px-2 py-0.5 rounded-full uppercase tracking-tighter shadow-sm border border-rose-100/50">
               Current: Tier 2
            </span>
            <LuArrowRight className="text-gray-300" size={12} />
            <span className="bg-gray-100 text-gray-500 text-[9px] font-black px-2 py-0.5 rounded-full uppercase tracking-tighter">
               Next: Elite
            </span>
          </div>
          <h1 className="text-4xl font-black text-gray-900 font-serif leading-none lowercase tracking-tighter">Verification Portal</h1>
          <p className="text-sm text-gray-500 font-sans mt-2">Scale your artisan business. Reach key milestones to unlock automated verification and lower platform fees.</p>
        </div>
        
        <div className="bg-white p-2 rounded-2xl shadow-sm border border-gray-100 flex items-center gap-3 pr-6">
           <div className="w-12 h-12 bg-gray-900 text-[#C4928F] rounded-xl flex items-center justify-center">
              <LuShieldCheck size={26} />
           </div>
           <div>
              <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none mb-1">Status</p>
              <h3 className="text-sm font-bold text-gray-900">Actively Verified</h3>
           </div>
        </div>
      </section>

      {/* Main Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        <MetricCard 
          label="Successful Trades" 
          current="76" 
          target="100" 
          progress={76} 
          icon={<LuCircleCheck size={24} />}
        />
        <MetricCard 
          label="Total Volume (USDC)" 
          current="12,500" 
          target="25,000" 
          progress={50} 
          icon={<LuTrendingUp size={24} />}
        />
      </div>

      <section className="grid grid-cols-1 lg:grid-cols-3 gap-8">
        {/* Tier Benefits Sidebar */}
        <div className="lg:col-span-1 bg-gray-900 rounded-[35px] p-10 text-white relative overflow-hidden flex flex-col justify-between group">
           <div>
              <LuBadgeCheck className="text-[#C4928F] text-5xl mb-8 group-hover:scale-110 transition-transform duration-500" />
              <h3 className="text-2xl font-black mb-6 font-serif">Unlocked Benefits</h3>
              <ul className="space-y-5">
                 <li className="flex items-center gap-3 text-sm group/item">
                    <LuZap className="text-[#C4928F] shrink-0" size={18} />
                    <span className="text-gray-400 group-hover/item:text-white transition-colors tracking-tight">2.5% Platform Fees (Reduced)</span>
                 </li>
                 <li className="flex items-center gap-3 text-sm group/item">
                    <LuZap className="text-[#C4928F] shrink-0" size={18} />
                    <span className="text-gray-400 group-hover/item:text-white transition-colors tracking-tight">Verified Badge on Product Cards</span>
                 </li>
                 <li className="flex items-center gap-3 text-sm group/item">
                    <LuZap className="text-[#C4928F] shrink-0" size={18} />
                    <span className="text-gray-400 group-hover/item:text-white transition-colors tracking-tight">Priority in "Featured" Sections</span>
                 </li>
                 <li className="flex items-center gap-3 text-sm opacity-30">
                    <LuZap className="text-gray-500 shrink-0" size={18} />
                    <span className="text-gray-600 line-through">0.5% Cash-back on Volume</span>
                 </li>
              </ul>
           </div>
           
           <div className="absolute top-[-30px] right-[-30px] w-60 h-60 bg-[#C4928F] opacity-[0.05] rounded-full blur-[80px] pointer-events-none group-hover:opacity-[0.08] transition-opacity" />
           
           <button className="mt-12 w-full bg-white/5 hover:bg-white/10 text-[#C4928F] text-[10px] font-black py-4 rounded-2xl border border-white/5 uppercase tracking-[0.2em] transition-all">
              View Tier Matrix
           </button>
        </div>

        {/* Manual Verification Request */}
        <div className="lg:col-span-2 bg-white rounded-3xl border border-gray-100 shadow-xl p-10 flex flex-col justify-center relative group min-h-[450px]">
           <div className="bg-rose-50/30 w-16 h-16 rounded-3xl flex items-center justify-center text-[#C4928F] mb-8 group-hover:rotate-6 transition-transform">
              <LuUserCheck size={32} />
           </div>
           
           <h2 className="text-3xl font-black text-gray-900 font-serif mb-4 leading-[1.1] tracking-tight">Need a manual review?</h2>
           <p className="text-base text-gray-500 leading-relaxed font-sans max-w-xl mb-10">
              If your business requires a higher tier or has unique circumstances (e.g., high-value items), our moderators can manually review your application.
           </p>
           
           <div className="grid grid-cols-2 gap-10 mb-10">
              <div className="space-y-1">
                 <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Typical response</p>
                 <p className="text-sm font-bold text-gray-900 italic font-mono uppercase tracking-tighter">48 - 72 Hours</p>
              </div>
              <div className="space-y-1 text-right">
                 <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Review Fee</p>
                 <p className="text-sm font-bold text-gray-900 italic font-mono uppercase tracking-tighter">Free for Tier 2</p>
              </div>
           </div>
           
           <button className="w-full bg-gray-900 text-white text-xs font-black py-5 rounded-3xl shadow-2xl shadow-gray-200 hover:bg-[#C4928F] hover:shadow-[#C4928F]/20 transition-all uppercase tracking-[0.25em]">
              Apply for Manual Review
           </button>
           
           {/* Visual background details */}
           <div className="absolute bottom-0 right-0 w-[150px] opacity-[0.02] pointer-events-none">
              <LuHistory size={150} />
           </div>
        </div>
      </section>
    </div>
  );
}

const LuArrowRight = ({ className, size }: { className?: string, size?: number }) => (
  <svg 
    xmlns="http://www.w3.org/2000/svg" 
    width={size || 24} 
    height={size || 24} 
    viewBox="0 0 24 24" 
    fill="none" 
    stroke="currentColor" 
    strokeWidth="2" 
    strokeLinecap="round" 
    strokeLinejoin="round" 
    className={className}
  >
    <line x1="5" y1="12" x2="19" y2="12"></line>
    <polyline points="12 5 19 12 12 19"></polyline>
  </svg>
);
