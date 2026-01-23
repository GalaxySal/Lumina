{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE FlexibleContexts #-}

module Main where

import System.IO (isEOF, getLine, hFlush, stdout)
import Text.Parsec
import Text.Parsec.String (Parser)
import qualified Data.Map as Map
import Control.Monad.State
import Data.List (intercalate)

-- ==========================================
-- Kip Type System (Semantic Intelligence)
-- ==========================================

-- Grammatical Cases (Isim Halleri)
data Case
    = Nominative    -- Yalın (Subject)
    | Accusative    -- Belirtme (Direct Object, e.g., 'i' hali)
    | Dative        -- Yönelme (Indirect Object, e.g., 'e' hali)
    | Locative      -- Bulunma (Location, e.g., 'de' hali)
    | Ablative      -- Ayrılma (Source, e.g., 'den' hali)
    | Instrumental  -- Vasıta (With/By, e.g., 'ile')
    deriving (Show, Eq, Ord)

-- Moods (Fiil Kipleri)
data Mood
    = Indicative    -- Haber Kipi (Facts)
    | Imperative    -- Emir Kipi (Commands)
    | Optative      -- İstek Kipi (Wishes/Requests)
    | Conditional   -- Şart Kipi (Conditions)
    deriving (Show, Eq)

-- AST Nodes
data Expr
    = Literal String Case           -- Data with Semantic Case
    | Variable String               -- Reference
    | Command String Mood [Expr]    -- Action with Mood and Arguments
    | Sequence [Expr]
    deriving (Show)

-- Evaluation Result
data Value
    = ValString String
    | ValVoid
    | ValError String
    deriving (Show)

-- Environment
type Env = Map.Map String Value

-- ==========================================
-- Lexer & Parser
-- ==========================================

lexer :: Parser String
lexer = many1 alphaNum

ws :: Parser ()
ws = skipMany (oneOf " \t")

parseCase :: Parser Case
parseCase = do
    char '['
    c <- many1 letter
    char ']'
    return $ case c of
        "Yalin"    -> Nominative
        "Belirtme" -> Accusative
        "Yonelme"  -> Dative
        "Bulunma"  -> Locative
        "Ayrilma"  -> Ablative
        "Vasita"   -> Instrumental
        _          -> Nominative -- Default

parseMood :: Parser Mood
parseMood = do
    char '<'
    m <- many1 letter
    char '>'
    return $ case m of
        "Haber" -> Indicative
        "Emir"  -> Imperative
        "Istek" -> Optative
        "Sart"  -> Conditional
        _       -> Indicative

parseLiteral :: Parser Expr
parseLiteral = do
    char '"'
    content <- many (noneOf "\"")
    char '"'
    ws
    c <- option Nominative parseCase
    return $ Literal content c

parseCommand :: Parser Expr
parseCommand = do
    cmd <- many1 letter
    ws
    m <- option Imperative parseMood
    ws
    args <- many parseLiteral
    return $ Command cmd m args

parseExpr :: Parser Expr
parseExpr = try parseCommand <|> parseLiteral

-- ==========================================
-- Semantic Interpreter
-- ==========================================

-- Validates if the arguments match the verb's required cases
validateSemantics :: String -> Mood -> [Expr] -> Maybe String
validateSemantics "yukle" Imperative [Literal _ Accusative] = Nothing -- 'yukle' needs Accusative
validateSemantics "yukle" Imperative [Literal _ c] = Just $ "Semantic Error: 'yukle' (Load) expects [Belirtme] (Accusative) object, found [" ++ show c ++ "]."
validateSemantics "git" Imperative [Literal _ Dative] = Nothing -- 'git' needs Dative
validateSemantics "git" Imperative [Literal _ c] = Just $ "Semantic Error: 'git' (Go) expects [Yonelme] (Dative) target, found [" ++ show c ++ "]."
validateSemantics _ _ _ = Nothing -- Allow others for now

eval :: Expr -> StateT Env IO Value
eval (Literal s _) = return $ ValString s
eval (Command cmd mood args) = do
    -- 1. Semantic Check
    case validateSemantics cmd mood args of
        Just err -> return $ ValError err
        Nothing  -> do
            -- 2. Execution
            liftIO $ putStrLn $ "Executing: " ++ cmd ++ " (" ++ show mood ++ ") with args: " ++ show args
            return ValVoid
eval _ = return ValVoid

-- ==========================================
-- Main Loop
-- ==========================================

runInput :: String -> IO ()
runInput input = do
    case parse parseExpr "kip" input of
        Left err -> putStrLn $ "Parse Error: " ++ show err
        Right ast -> do
            putStrLn $ "AST: " ++ show ast
            (val, _) <- runStateT (eval ast) Map.empty
            case val of
                ValError e -> putStrLn $ "RUNTIME ERROR: " ++ e
                _          -> putStrLn "OK"

main :: IO ()
main = do
    putStrLn "Kip Semantic Intelligence (Haskell) v0.3.0"
    putStrLn "Ready. Waiting for semantic input..."
    hFlush stdout
    loop

loop :: IO ()
loop = do
    done <- isEOF
    if done
        then return ()
        else do
            input <- getLine
            if input == "exit"
                then return ()
                else do
                    runInput input
                    hFlush stdout
                    loop
