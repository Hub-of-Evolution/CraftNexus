"use client";

import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { signupSchema } from "@/lib/validations/auth";
import Image from "next/image";
import Link from "next/link";

export default function SignupPage() {
  const { register, handleSubmit, formState: { errors } } = useForm({
    resolver: zodResolver(signupSchema),
  });

  const onSubmit = (data: any) => {
    console.log("Form Data:", data);
  };

  return (
    <div className="flex min-h-screen bg-white">
      {/* Left side - Illustration Area */}
      <div className="hidden lg:flex w-1/2 flex-col items-center justify-center p-12 bg-white">
        <div className="max-w-xl text-left">
          <div className="mb-12">
             <Image src="/logo.png" width={120} height={50} alt="CraftNexus" priority />
          </div>
          
          <div className="relative w-[500px] h-[500px] mb-10">
            <Image 
              src="/artisan-illustration.png" 
              fill
              style={{ objectFit: 'contain' }}
              alt="Artisan working on pottery" 
            />
          </div>

          <p className="text-[#8B8B8B] uppercase tracking-widest text-xs font-semibold mb-2">
            Join as an Artisan
          </p>
          <h1 className="text-5xl font-bold text-[#1A1A1A] leading-tight mb-6">
            Turn Your Craft Into <br /> Income
          </h1>
          <p className="text-[#666666] text-lg max-w-md leading-relaxed">
            Create your shop, showcase your homemade products, and reach customers worldwide.
          </p>
        </div>
      </div>

      {/* Right side - Form Card */}
      <div className="w-full lg:w-1/2 flex items-center justify-center p-6 bg-[#F9F9F9]">
        <div className="w-full max-w-[550px] bg-white p-10 rounded-3xl shadow-xl shadow-black/5">
          <header className="mb-8">
            <h2 className="text-2xl font-bold text-[#1A1A1A]">Create Your Account</h2>
            <p className="text-sm text-[#8B8B8B] mt-1">Start your artisan journey today</p>
          </header>
          
          <form onSubmit={handleSubmit(onSubmit)} className="space-y-5">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              <Input label="Full Name" placeholder="Full name" {...register("fullName")} error={errors.fullName?.message} />
              <Input label="Email Address" placeholder="Email address" {...register("email")} error={errors.email?.message} />
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              <Input label="Phone Number" placeholder="Phone number" {...register("phone")} error={errors.phone?.message} />
              <div className="flex flex-col">
                <label className="text-[13px] font-semibold text-[#1A1A1A] mb-1.5 ml-1">Craft Category</label>
                <select 
                  {...register("craftCategory")} 
                  className="w-full border border-[#E5E5E5] rounded-xl p-3 text-sm focus:ring-2 focus:ring-[#C48C8C] outline-none bg-white appearance-none"
                  style={{ backgroundImage: 'url("data:image/svg+xml,...")', backgroundRepeat: 'no-repeat', backgroundPosition: 'right 1rem center' }}
                >
                  <option value="">Select your craft</option>
                  <option value="pottery">Pottery</option>
                  <option value="weaving">Weaving</option>
                  <option value="jewelry">Jewelry</option>
                </select>
                {errors.craftCategory && <span className="text-[10px] text-red-500 mt-1 ml-1">{errors.craftCategory.message as string}</span>}
              </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              <Input label="Business Name" placeholder="Business name" {...register("businessName")} />
              <Input label="Location" placeholder="City, Country" {...register("location")} />
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
              <Input type="password" label="Password" placeholder="Password" {...register("password")} error={errors.password?.message} />
              <Input type="password" label="Confirm Password" placeholder="Confirm password" {...register("confirmPassword")} error={errors.confirmPassword?.message} />
            </div>

            <div className="flex flex-col">
              <label className="text-[13px] font-semibold text-[#1A1A1A] mb-1.5 ml-1">What makes your craft special?</label>
              <textarea 
                placeholder="Share your story, techniques, or inspiration..." 
                className="w-full border border-[#E5E5E5] rounded-xl p-3 text-sm h-28 focus:ring-2 focus:ring-[#C48C8C] outline-none resize-none"
                {...register("bio")}
              />
            </div>

            <div className="flex items-center space-x-2 py-2">
              <input type="checkbox" id="terms" className="w-4 h-4 accent-[#C48C8C] cursor-pointer" />
              <label htmlFor="terms" className="text-xs text-[#666666]">
                I agree to the <Link href="/terms" className="text-[#C48C8C] font-semibold underline">Terms & Conditions</Link>
              </label>
            </div>

            <button className="w-full bg-[#BC8B83] text-white py-4 rounded-xl font-bold text-sm shadow-lg shadow-[#BC8B83]/20 hover:bg-[#a67a73] transition-all transform active:scale-[0.98]">
              Create Artisan Account
            </button>

            <p className="text-center text-xs text-[#666666] mt-6">
              Already have an account?{" "}
              <Link href="/login" className="text-[#C48C8C] font-bold hover:underline">
                Sign in
              </Link>
            </p>
          </form>
        </div>
      </div>
    </div>
  );
}

// Reusable Input Component
const Input = ({ label, error, ...props }: any) => (
  <div className="flex flex-col w-full">
    <label className="text-[13px] font-semibold text-[#1A1A1A] mb-1.5 ml-1">{label}</label>
    <input 
      className="w-full border border-[#E5E5E5] rounded-xl p-3 text-sm placeholder:text-[#BFBFBF] focus:ring-2 focus:ring-[#C48C8C] outline-none transition-all" 
      {...props} 
    />
    {error && <span className="text-[10px] text-red-500 mt-1 ml-1">{error}</span>}
  </div>
);