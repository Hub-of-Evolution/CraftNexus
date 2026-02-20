import Link from "next/link";

export default function NotFound() {
  return (
    <main className="flex flex-col items-center justify-center min-h-[80vh] px-4 py-20 relative overflow-hidden bg-white font-inter">
      {/* Background 404 Text */}
      <div className="absolute inset-0 flex items-center justify-center pointer-events-none select-none z-0">
        <span className="text-[180px] sm:text-[300px] md:text-[400px] lg:text-[500px] font-bold text-[#F2E4E3] leading-none">
          404
        </span>
      </div>

      {/* Content */}
      <div className="relative z-10 flex flex-col items-center text-center max-w-2xl mx-auto">
        <h1 className="text-4xl sm:text-5xl md:text-[64px] font-bold text-[#272727] mb-4 tracking-tight leading-tight">
          Page Not Found
        </h1>
        <p className="text-base sm:text-lg md:text-xl text-[#272727] mb-10 max-w-lg leading-relaxed">
          Oops seems like this page you are looking for does not exist.
        </p>

        <div className="flex flex-wrap justify-center gap-6">
          <Link
            href="/"
            style={{ boxShadow: "5px 5px 4px 0px #00000040" }}
            className="px-8 py-3 rounded-lg text-lg sm:text-xl font-medium text-white bg-[#C4928F] hover:opacity-90 transition-all transform hover:-translate-y-0.5 active:translate-y-0"
          >
            Back to Home
          </Link>
          <Link
            href="/market"
            style={{ boxShadow: "5px 5px 4px 0px #00000040" }}
            className="px-8 py-3 rounded-lg text-lg sm:text-xl font-medium text-white bg-[#517A77] hover:opacity-90 transition-all transform hover:-translate-y-0.5 active:translate-y-0"
          >
            Visit Market
          </Link>
        </div>
      </div>
    </main>
  );
}
