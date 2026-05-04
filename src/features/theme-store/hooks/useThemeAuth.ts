import { useState, useEffect, useCallback } from "react";
import * as api from "../api";

export function useThemeAuth() {
  const [token, setToken] = useState<string | null>(null);
  const [username, setUsername] = useState<string | null>(null);

  useEffect(() => {
    const savedToken = localStorage.getItem("tiez_theme_store_token");
    const savedUser = localStorage.getItem("tiez_theme_store_username");
    if (savedToken && savedUser) {
      setToken(savedToken);
      setUsername(savedUser);
    }
  }, []);

  const handleLogin = useCallback(
    async (user: string, password: string) => {
      const result = await api.login(user, password);
      localStorage.setItem("tiez_theme_store_token", result.token);
      localStorage.setItem("tiez_theme_store_username", result.username);
      setToken(result.token);
      setUsername(result.username);
      return result;
    },
    []
  );

  const handleRegister = useCallback(
    async (user: string, password: string) => {
      const result = await api.register(user, password);
      localStorage.setItem("tiez_theme_store_token", result.token);
      localStorage.setItem("tiez_theme_store_username", result.username);
      setToken(result.token);
      setUsername(result.username);
      return result;
    },
    []
  );

  const handleLogout = useCallback(() => {
    localStorage.removeItem("tiez_theme_store_token");
    localStorage.removeItem("tiez_theme_store_username");
    setToken(null);
    setUsername(null);
  }, []);

  return {
    isLoggedIn: !!token,
    username,
    login: handleLogin,
    register: handleRegister,
    logout: handleLogout,
  };
}
