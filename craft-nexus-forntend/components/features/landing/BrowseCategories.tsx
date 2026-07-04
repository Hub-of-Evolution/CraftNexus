"use client";
import Image from "next/image";
import { useState, useEffect, useRef } from "react";

const categories = [
    {
        id: "sculpting",
        name: "Sculpting",
        meta: "28 Courses / 600 Arts",
        color: "#517A77",
        icon: "./vase.svg",
    },
    {
        id: "painting",
        name: "Painting",
        meta: "20 Courses / 1k Arts",
        color: "#C4928F",
        icon: "./drawing.svg",
    },
    {
        id: "crocheting",
        name: "Crocheting",
        meta: "19 Courses / 5k Art",
        color: "#517A77",
        icon: "./needle-yarn.svg",
    },
    {
        id: "sewing",
        name: "Sewing",
        meta: "19 Courses",
        color: "#517A77",
        icon: "./sewing-machine.svg",
    },
    {
        id: "sculpting2",
        name: "Sculpting",
        meta: "28 Courses / 600 Arts",
        color: "#517A77",
        icon: "./vase.svg",
    },
    {
        id: "painting2",
        name: "Painting",
        meta: "20 Courses / 1k Arts",
        color: "#C4928F",
        icon: "./drawing.svg",
    },
];

// How many cards to show per breakpoint
const COLS = { mobile: 1, sm: 2, lg: 4 };

export default function CategorySection() {
    const [startIndex, setStartIndex] = useState(0);
    const [inView, setInView] = useState(false);
    const [animKey, setAnimKey] = useState(0);
    const [visibleCount, setVisibleCount] = useState(COLS.lg);
    const sectionRef = useRef(null);

    // Track viewport width to know how many cards are visible
    useEffect(() => {
        const update = () => {
            if (window.innerWidth < 640) setVisibleCount(COLS.mobile);
            else if (window.innerWidth < 1024) setVisibleCount(COLS.sm);
            else setVisibleCount(COLS.lg);
        };
        update();
        window.addEventListener("resize", update);
        return () => window.removeEventListener("resize", update);
    }, []);

    // Intersection observer — fires once
    useEffect(() => {
        const observer = new IntersectionObserver(
            ([entry]) => {
                if (entry.isIntersecting) {
                    setInView(true);
                    observer.disconnect();
                }
            },
            { threshold: 0.15 }
        );
        if (sectionRef.current) observer.observe(sectionRef.current);
        return () => observer.disconnect();
    }, []);

    const prev = () => {
        setStartIndex((i) => (i - 1 + categories.length) % categories.length);
        setAnimKey((k) => k + 1);
    };
    const next = () => {
        setStartIndex((i) => (i + 1) % categories.length);
        setAnimKey((k) => k + 1);
    };

    const visible = Array.from({ length: visibleCount }, (_, i) =>
        categories[(startIndex + i) % categories.length]
    );

    return (
        <section
            ref={sectionRef}
            className="bg-[#FBEDE0] py-8 sm:py-10 lg:py-12 pb-12 sm:pb-14 font-sans overflow-hidden"
        >
            {/* Title */}
            <h2
                className={`font-serif text-3xl sm:text-4xl font-bold mx-4 sm:ml-10 lg:ml-18 mb-8 sm:mb-10 transition-opacity ${inView ? "animate-fadeSlideDown" : "opacity-0"
                    }`}
                style={{ animationDelay: "0ms" }}
            >
                Browse Categories
            </h2>

            {/* Carousel row */}
            <div className="flex items-center mb-10 md:mb-20">

                {/* ── Left arrow ── */}
                <button
                    className={`shrink-0 w-8 h-8 sm:w-[42px] sm:h-[42px] rounded-full border-2 border-[#517A77] bg-transparent flex items-center justify-center text-[#517A77] transition-all hover:bg-[#517A77] hover:text-white hover:scale-110 active:scale-95 mt-7 ml-2 sm:ml-4 mr-2 sm:mr-[14px] ${inView ? "animate-fadeSlideUp" : "opacity-0"
                        }`}
                    style={{ animationDelay: "100ms" }}
                    onClick={prev}
                    aria-label="Previous category"
                >
                    <svg width="12" height="12" className="sm:w-[14px] sm:h-[14px]" viewBox="0 0 14 14" fill="none">
                        <path d="M9 2L4 7l5 5" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                </button>

                <div className="flex-1 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 sm:gap-5 min-w-0">
                    {visible.map((cat, idx) => {
                        const cardDelay = inView ? 150 + idx * 90 : 0;
                        const iconDelay = inView ? 200 + idx * 90 : 0;
                        return (
                            <div
                                className={`relative pt-8 group ${inView ? "animate-cardSlideIn" : "opacity-0"
                                    }`}
                                key={`${cat.id}-${animKey}-${idx}`}
                                style={{ animationDelay: `${cardDelay}ms` }}
                            >
                                {/* Icon bubble */}
                                <div
                                    className={`absolute top-0 left-1/2 -translate-x-1/2 w-14 h-14 sm:w-16 sm:h-16 rounded-full bg-[#B1D4D6] flex items-center justify-center z-20 shadow-[0_4px_14px_rgba(0,0,0,0.25)] pointer-events-none transition-transform duration-250 group-hover:-translate-y-1.5 ${inView ? "animate-popIn" : "opacity-0"
                                        }`}
                                    style={{ animationDelay: `${iconDelay}ms` }}
                                >
                                    <Image
                                        src={cat.icon}
                                        width={28}
                                        height={28}
                                        className="sm:w-8 sm:h-8"
                                        alt={cat.name}
                                    />
                                </div>

                                {/* Card */}
                                <div className="relative">
                                    {/* Offset shadow — pure Tailwind */}
                                    <div
                                        className="absolute inset-0 translate-x-1.5 translate-y-1.5 rounded-lg opacity-50 -z-10"
                                        style={{ backgroundColor: cat.color }}
                                    />
                                    <a
                                        href={`/category/${cat.id}`}
                                        className="relative z-10 block rounded-lg pt-10 sm:pt-11 px-4 sm:px-5 pb-5 sm:pb-6 text-center transition-all duration-[220ms] hover:-translate-y-1 hover:shadow-[0_14px_34px_rgba(0,0,0,0.4)]"
                                        style={{ backgroundColor: cat.color }}
                                    >
                                        <p className="font-serif text-base sm:text-lg font-bold text-white mb-1.5 sm:mb-2">
                                            {cat.name}
                                        </p>
                                        {cat.meta && (
                                            <p className="text-xs sm:text-sm text-white/75 font-medium tracking-wide">
                                                {cat.meta}
                                            </p>
                                        )}
                                    </a>
                                </div>
                            </div>
                        );
                    })}
                </div>

                {/* ── Right arrow ── */}
                <button
                    className={`shrink-0 w-8 h-8 sm:w-[42px] sm:h-[42px] rounded-full border-2 border-[#517A77] bg-transparent flex items-center justify-center text-[#517A77] transition-all hover:bg-[#517A77] hover:text-white hover:scale-110 active:scale-95 mt-7 ml-2 sm:ml-[14px] mr-2 sm:mr-4 ${inView ? "animate-fadeSlideUp" : "opacity-0"
                        }`}
                    style={{ animationDelay: "560ms" }}
                    onClick={next}
                    aria-label="Next category"
                >
                    <svg width="12" height="12" className="sm:w-[14px] sm:h-[14px]" viewBox="0 0 14 14" fill="none">
                        <path d="M5 2l5 5-5 5" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                </button>

            </div>
        </section>
    );
}