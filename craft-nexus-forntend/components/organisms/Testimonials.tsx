"use client";

import { useEffect, useState } from "react";
import { TestimonialCard, type Testimonial } from "@/components/molecules/TestimonialCard";

const TESTIMONIALS: Testimonial[] = [
  {
    id: 1,
    name: "Nino",
    role: "Ceramic sells from Nina from Canada",
    quote:
      "As a local artisan skilled in ceramics I was struggling to sell my art work at fair prices but now I have a wider market and a fair price for my arts thanks to craft nexus.",
    avatar: "/Intersect.png",
    rating: 5,
  },
  {
    id: 2,
    name: "Tako",
    role: "Alumna of Pottery Course by Glimmeria",
    quote:
      "This was a transformative experience that allowed me to unleash my creativity and develop a deep appreciation for the art of ceramics.",
    avatar: "/Intersect (1).png",
    rating: 5,
  },
  {
    id: 3,
    name: "Maria",
    role: "Jewelry Designer from Spain",
    quote:
      "CraftNexus opened doors I didn't know existed. The community, the workshops, and the platform itself are all exceptional. I've doubled my sales since joining.",
    avatar: "/Intersect (1).png",
    rating: 5,
  },
  {
    id: 4,
    name: "James",
    role: "Woodworker & Furniture Maker",
    quote:
      "The mentorship I received through CraftNexus was invaluable. I went from hobbyist to professional in under six months. Truly life-changing.",
    avatar: "/Intersect.png",
    rating: 5,
  },
];

function ArrowButton({
  direction,
  disabled,
  onClick,
}: {
  direction: "prev" | "next";
  disabled: boolean;
  onClick: () => void;
}) {
  const base =
    "shrink-0 w-9 h-9 sm:w-11 sm:h-11 flex items-center justify-center rounded-full text-3xl sm:text-4xl transition-colors duration-200 select-none leading-none";
  const active =
    "text-[#5a3535] hover:text-[#C0404A] hover:bg-[#C0404A]/10 cursor-pointer";
  const inactive = "text-[#C0404A] cursor-default pointer-events-none";

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      aria-label={direction === "prev" ? "Previous testimonials" : "Next testimonials"}
      className={`${base} ${disabled ? inactive : active}`}
    >
      {direction === "prev" ? "‹" : "›"}
    </button>
  );
}

export default function TestimonialsSection() {
  const [cardsPerPage, setCardsPerPage] = useState(2);
  const [page, setPage] = useState(0);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 639px)");

    const update = (e: MediaQueryListEvent | MediaQueryList) => {
      const mobile = e.matches;
      setCardsPerPage(mobile ? 1 : 2);
      setPage(0);
    };

    update(mq); 
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  const totalPages = Math.ceil(TESTIMONIALS.length / cardsPerPage);
  const visible = TESTIMONIALS.slice(page * cardsPerPage, (page + 1) * cardsPerPage);

  return (
    <section
      id="testimonials"
      aria-labelledby="testimonials-heading"
      className="w-full bg-[#D6B3B1] flex flex-col items-center py-16 md:py-20 px-2 sm:px-6 lg:px-12 relative overflow-hidden"
    >
      

      {/* Heading */}
      <h2
        id="testimonials-heading"
        className="font-serif text-[2rem] sm:text-[2.15rem] md:text-[2.5rem] font-bold text-[#272727] mb-14 md:mb-16 text-center tracking-tight"
      >
        Testimonials
      </h2>

      {/* ── Carousel ── */}
      <div className="flex items-center gap-2 sm:gap-4 w-full max-w-3xl lg:max-w-4xl">
        <ArrowButton
          direction="prev"
          disabled={page === 0}
          onClick={() => setPage((p) => Math.max(0, p - 1))}
        />

       
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-5 md:gap-7 flex-1 min-w-0">
          {visible.map((t) => (
            <TestimonialCard key={t.id} testimonial={t} />
          ))}
        </div>

        <ArrowButton
          direction="next"
          disabled={page === totalPages - 1}
          onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
        />
      </div>


      {/* Join Workshop CTA */}
      <button
        type="button"
        className="mt-10 px-11 py-3.5 rounded-[10px] bg-[#517A77] text-white text-sm font-semibold tracking-wide hover:bg-[#2e5350] hover:-translate-y-0.5 active:translate-y-0 transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#3d6b68] focus-visible:ring-offset-2 shadow-[0_20px_20px_rgba(100,40,40,0.22)] "
      >
        Join Workshop
      </button>
    </section>
  );
}