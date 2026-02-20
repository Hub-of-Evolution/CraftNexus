import { StarRating } from "./StarRating";
import { TestimonialAvatar } from "./TestimonialAvatar";

export interface Testimonial {
  id: number;
  name: string;
  role: string;
  quote: string;
  avatar: string;
  rating: number;
}

interface TestimonialCardProps {
  testimonial: Testimonial;
}


export function TestimonialCard({ testimonial }: TestimonialCardProps) {
  const { name, role, quote, avatar, rating } = testimonial;

  return (
 
    <div className="relative mt-12 bg-[#f5e8e4] rounded-[20px] shadow-[0_20px_20px_rgba(100,40,40,0.22)] flex flex-col items-center pt-16 pb-8 px-7 gap-2.5">
      <TestimonialAvatar src={avatar} name={name} />

      <div className="absolute top-14 right-5 sm:top-4 ">
        <StarRating rating={rating} />
      </div>

      {/* Name */}
      <h3 className="font-serif text-[1.1rem] font-bold text-[#2d1a1a] text-center leading-tight m-0 sm:mb-6">
        {name}
      </h3>

      {/* Role / subtitle */}
      <p className="text-[0.78rem] italic text-[#6b4545] text-center leading-snug m-0">
        {role}
      </p>

      {/* Quote body */}
      <div className="relative w-full mt-1.5">
        {/* Decorative open-quote glyph */}
        <span
          aria-hidden="true"
          className="absolute -top-1 -left-1 text-[2.5rem] leading-none font-serif text-[#b05555] opacity-70 select-none"
        >
          &ldquo;
        </span>
        <p className="text-[0.82rem] leading-relaxed text-[#4a3535] pl-6 text-left m-0">
          {quote}
        </p>
      </div>
    </div>
  );
}
