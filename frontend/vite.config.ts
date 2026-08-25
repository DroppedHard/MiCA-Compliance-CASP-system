import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"
export default defineConfig({plugins:[react()],server:{port:5174,proxy:{"/api":"http://127.0.0.1:3200","/health":"http://127.0.0.1:3200"}}})
