"use client";

import Image from "next/image";

const stats = [
    { value: "30K", label: "Students" },
    { value: "180", label: "Courses" },
    { value: "100k", label: "Sold Art" },
];

const brands = [
    { id: "glimmerea", icon: "/glimmerea.svg", label: "glimmerea" },
    { id: "handicraft", icon: "/handicraft.svg", label: "Handicraft" },
    { id: "scissors", icon: "/scissors.svg", label: "" },
    { id: "avase", icon: "/avase.svg", label: "Avase" },
];

export default function HeroSection() {
    return (
        <section className="w-full rounded-3xl px-4 sm:px-6 md:px-10 py-8 md:py-16 lg:max-w-7xl mx-auto">
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 lg:gap-12 items-center">
                {/* LEFT COLUMN */}
                <div className="space-y-6 md:space-y-8">
                    <h1 className="text-3xl text-center md:text-start sm:text-4xl md:text-5xl lg:text-[64px] font-bold text-[#272727] leading-tight">
                        Sell and Learn<br />Handcraft
                    </h1>

                    <p className="text-base text-center md:text-start sm:text-lg md:text-xl text-[#272727] leading-relaxed max-w-2xl">
                        Sell a craft on our market place or select desired courses from online
                        or on-site categories.
                    </p>

                    {/* CTA Buttons */}
                    <div className="flex flex-wrap justify-center md:justify-start gap-4 sm:gap-6 md:gap-10">
                        <button
                            style={{ boxShadow: "5px 5px 4px 0px #00000040" }}
                            className="px-4 sm:px-6 py-2 sm:py-3 rounded-lg text-sm sm:text-base md:text-lg lg:text-2xl font-medium text-white bg-[#C4928F] hover:opacity-85 transition-opacity"
                        >
                            Buy Handcraft
                        </button>
                        <button
                            className="px-4 sm:px-6 py-2 sm:py-3 rounded-lg text-sm sm:text-base md:text-lg lg:text-2xl font-medium text-white bg-[#517A77] hover:opacity-85 transition-opacity"
                            style={{ boxShadow: "5px 5px 4px 0px #00000040" }}
                        >
                            Sell Handcraft
                        </button>
                    </div>

                    {/* Stats */}
                    <div className="flex flex-wrap justify-center md:justify-start gap-6 sm:gap-10 md:gap-16 lg:gap-20 mt-8 md:mt-12">
                        {stats.map(({ value, label }) => (
                            <div key={label}>
                                <div className="text-2xl sm:text-3xl md:text-4xl lg:text-[40px] font-bold text-[#D6B3B1]">
                                    {value}
                                </div>
                                <div className="text-xs sm:text-sm md:text-base lg:text-xl text-black mt-0.5">
                                    {label}
                                </div>
                            </div>
                        ))}
                    </div>
                </div>

                {/* RIGHT COLUMN */}
                <div className="flex flex-col items-center justify-start space-y-6">
                    <div className="w-full max-w-sm sm:max-w-md md:max-w-lg lg:max-w-xl mx-auto">
                        <Image
                            src="/hero-image.png"
                            alt="Handcraft instructor"
                            width={500}
                            height={500}
                            className="w-full h-auto object-contain"
                            priority
                        />
                    </div>
                </div>
            </div>

            {/* BOTTOM SECTION (text + brands) */}
            <div className="flex flex-col sm:flex-row items-center gap-4 sm:gap-6 md:gap-8 mt-6 md:mt-8">
                <p className="text-sm sm:text-base md:text-lg lg:text-xl text-[#272727] text-center sm:text-left">
                    Learn from Top handcraft instructors and course providers
                </p>
                <div className="flex items-center justify-center gap-4 sm:gap-6 md:gap-8 flex-wrap">
                    {brands.map(({ id, icon, label }) => (
                        <div key={id} className="flex flex-col items-center gap-1 opacity-75">
                            <Image
                                src={icon}
                                alt={label || id}
                                width={60}
                                height={40}
                                className="object-contain w-auto h-8 sm:h-10 md:h-20"
                            />
                        </div>
                    ))}
                </div>
            </div>

            {/* Animation styles (if needed) */}
            <style jsx>{`
        @keyframes twinkle {
          0% { opacity: 0.5; transform: scale(0.9) rotate(-5deg); }
          100% { opacity: 1; transform: scale(1.1) rotate(5deg); }
        }
        .animate-twinkle {
          animation: twinkle 2s ease-in-out infinite alternate;
        }
      `}</style>
        </section>
    );
}