import { AnimatePresence, motion } from "framer-motion";
import type { ToastItem } from "../types";

interface ToastContainerProps {
  toasts: ToastItem[];
}

const ToastContainer = ({ toasts }: ToastContainerProps) => (
  <div className="toast-container">
    <AnimatePresence>
      {toasts.map((toast) => (
        <motion.div
          key={toast.id}
          initial={{ opacity: 0, y: 20, scale: 0.9 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, scale: 0.9, transition: { duration: 0.2 } }}
          className="toast-item"
        >
          {toast.msg}
        </motion.div>
      ))}
    </AnimatePresence>
  </div>
);

export default ToastContainer;
