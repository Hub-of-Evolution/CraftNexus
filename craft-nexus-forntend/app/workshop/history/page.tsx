"use client";

import React from "react";
import { 
  LuHistory, 
  LuSearch, 
  LuFilter, 
  LuDownload, 
  LuChevronRight, 
  LuCircleCheck, 
  LuCircleX, 
  LuRotateCcw 
} from "react-icons/lu";

interface TransactionRowProps {
  id: string;
  date: string;
  amount: string;
  buyer: string;
  status: "Completed" | "Refunded" | "Disputed";
}

const TransactionRow = ({ id, date, amount, buyer, status }: TransactionRowProps) => {
  const statusStyles = {
    Completed: "bg-green-50 text-green-600 border-green-100",
    Refunded: "bg-rose-50 text-rose-600 border-rose-100",
    Disputed: "bg-amber-50 text-amber-600 border-amber-100",
  };

  const StatusIcon = {
    Completed: LuCircleCheck,
    Refunded: LuCircleX,
    Disputed: LuRotateCcw,
  }[status];

  return (
    <div className="bg-white px-6 py-5 rounded-2xl border border-gray-100 hover:border-[#C4928F]/30 hover:shadow-sm transition-all flex items-center justify-between group cursor-pointer">
      <div className="flex items-center gap-6 flex-1">
        <div className={`w-10 h-10 rounded-xl border flex items-center justify-center shrink-0 ${statusStyles[status]}`}>
          <StatusIcon size={20} />
        </div>
        
        <div className="flex-1 grid grid-cols-1 md:grid-cols-4 gap-4 items-center">
          <div className="space-y-0.5">
            <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Escrow ID</p>
            <p className="text-sm font-bold text-gray-900 font-mono tracking-tighter">#{id}</p>
          </div>
          <div className="space-y-0.5">
            <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Date</p>
            <p className="text-sm font-medium text-gray-600">{date}</p>
          </div>
          <div className="space-y-0.5">
            <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Counterparty</p>
            <p className="text-sm font-bold text-gray-900 italic lowercase tracking-tight">@{buyer}</p>
          </div>
          <div className="space-y-0.5">
            <p className="text-[10px] font-black text-gray-400 uppercase tracking-widest leading-none">Amount</p>
            <p className="text-sm font-black text-gray-900">{amount} <span className="text-[10px] text-gray-400">USDC</span></p>
          </div>
        </div>
      </div>
      
      <div className="flex items-center gap-4 pl-6 border-l border-gray-50">
        <span className={`text-[10px] font-black px-3 py-1 rounded-full uppercase tracking-tighter shadow-sm border ${statusStyles[status]}`}>
          {status}
        </span>
        <LuChevronRight className="text-gray-300 group-hover:text-[#C4928F] group-hover:translate-x-1 transition-all" size={20} />
      </div>
    </div>
  );
};

export default function HistoryPage() {
  const transactions: TransactionRowProps[] = [
    { id: "8421", date: "Mar 25, 2026", amount: "1,200.00", buyer: "alice_crypto", status: "Completed" },
    { id: "8418", date: "Mar 22, 2026", amount: "450.00", buyer: "bob_hands", status: "Refunded" },
    { id: "8415", date: "Mar 20, 2026", amount: "890.00", buyer: "charlie_dev", status: "Completed" },
    { id: "8402", date: "Mar 18, 2026", amount: "2,500.00", buyer: "danielle_w", status: "Disputed" },
    { id: "8395", date: "Mar 15, 2026", amount: "320.00", buyer: "eve_crafts", status: "Completed" },
    { id: "8388", date: "Mar 12, 2026", amount: "1,100.00", buyer: "frank_stellar", status: "Completed" },
  ];

  return (
    <div className="space-y-8 pb-10">
      {/* Header */}
      <section className="flex flex-col md:flex-row justify-between items-start md:items-center gap-6">
        <div className="space-y-1">
          <h1 className="text-4xl font-black text-gray-900 font-serif leading-none lowercase tracking-tighter">Transaction History</h1>
          <p className="text-sm text-gray-500 font-sans mt-2">Audit and track every settlement across your artisan journey.</p>
        </div>
        
        <button className="flex items-center gap-2 bg-white px-5 py-3 rounded-2xl border border-gray-100 shadow-sm hover:shadow-md transition-all text-[10px] font-black uppercase tracking-widest text-gray-600">
          <LuDownload size={16} className="text-[#C4928F]" />
          Export CSV
        </button>
      </section>

      {/* Filters Bar */}
      <section className="bg-white p-4 rounded-3xl border border-gray-100 shadow-sm flex flex-col md:flex-row gap-4">
        <div className="flex-1 relative group">
          <LuSearch className="absolute left-4 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-[#C4928F] transition-colors" size={18} />
          <input 
            type="text" 
            placeholder="Search by ID or buyer..." 
            className="w-full bg-gray-50 border-0 outline-0 py-3 pl-12 pr-4 rounded-2xl text-sm font-medium focus:ring-1 focus:ring-[#C4928F]/20 transition-all"
          />
        </div>
        <div className="flex gap-2">
           <button className="px-6 py-3 bg-gray-50 rounded-2xl text-[10px] font-black uppercase tracking-widest text-gray-500 hover:bg-gray-100 transition-colors flex items-center gap-2 border border-gray-100/50">
             <LuFilter size={16} /> 
             Status
           </button>
           <button className="px-6 py-3 bg-gray-900 text-white rounded-2xl text-[10px] font-black uppercase tracking-widest hover:bg-[#C4928F] transition-colors shadow-lg shadow-gray-200">
             Clear Filters
           </button>
        </div>
      </section>

      {/* Transactions List */}
      <section className="space-y-3">
        {transactions.map((tx) => (
          <TransactionRow key={tx.id} {...tx} />
        ))}
      </section>

      {/* Pagination */}
      <section className="flex justify-center pt-6">
        <nav className="flex items-center gap-2">
           <button className="w-10 h-10 rounded-xl border border-gray-100 flex items-center justify-center text-gray-400 hover:bg-gray-50 transition-colors">1</button>
           <button className="w-10 h-10 rounded-xl bg-gray-900 text-white flex items-center justify-center font-bold">2</button>
           <button className="w-10 h-10 rounded-xl border border-gray-100 flex items-center justify-center text-gray-400 hover:bg-gray-50 transition-colors">3</button>
           <span className="text-gray-300 px-2 text-xs">...</span>
           <button className="w-10 h-10 rounded-xl border border-gray-100 flex items-center justify-center text-gray-400 hover:bg-gray-50 transition-colors font-bold">12</button>
        </nav>
      </section>
    </div>
  );
}
