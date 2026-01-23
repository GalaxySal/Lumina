package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"strings"
	"sync"
	"time"
)

// ProtocolRequest represents a request from the main Tauri process
type ProtocolRequest struct {
	Command string          `json:"command"`
	Payload json.RawMessage `json:"payload"`
}

// ProtocolResponse represents a response to the main Tauri process
type ProtocolResponse struct {
	Status  string      `json:"status"`
	Message string      `json:"message,omitempty"`
	Data    interface{} `json:"data,omitempty"`
}

// ServerState holds the state of our network services
type ServerState struct {
	Listeners map[string]net.Listener
	Mutex     sync.Mutex
}

var state = ServerState{
	Listeners: make(map[string]net.Listener),
}

func main() {
	reader := bufio.NewReader(os.Stdin)
	writer := json.NewEncoder(os.Stdout)

	// Log startup
	fmt.Fprintln(os.Stderr, "Lumina Net (Go) Service Started")

	for {
		line, err := reader.ReadString('\n')
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error reading stdin: %v\n", err)
			break
		}

		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		var req ProtocolRequest
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			sendError(writer, "Invalid JSON format")
			continue
		}

		handleRequest(req, writer)
	}
}

func handleRequest(req ProtocolRequest, writer *json.Encoder) {
	switch req.Command {
	case "start_server":
		handleStartServer(req.Payload, writer)
	case "stop_server":
		handleStopServer(req.Payload, writer)
	case "status":
		handleStatus(writer)
	case "ping":
		writer.Encode(ProtocolResponse{Status: "ok", Message: "pong"})
	default:
		sendError(writer, "Unknown command: "+req.Command)
	}
}

type StartServerPayload struct {
	Port int    `json:"port"`
	Type string `json:"type"` // "tcp", "udp"
}

func handleStartServer(payload json.RawMessage, writer *json.Encoder) {
	var p StartServerPayload
	if err := json.Unmarshal(payload, &p); err != nil {
		sendError(writer, "Invalid payload for start_server")
		return
	}

	addr := fmt.Sprintf(":%d", p.Port)
	
	state.Mutex.Lock()
	defer state.Mutex.Unlock()

	if _, exists := state.Listeners[addr]; exists {
		sendError(writer, fmt.Sprintf("Server already running on %s", addr))
		return
	}

	ln, err := net.Listen("tcp", addr)
	if err != nil {
		sendError(writer, fmt.Sprintf("Failed to bind %s: %v", addr, err))
		return
	}

	state.Listeners[addr] = ln

	// Start accepting connections in a goroutine
	go func(listener net.Listener) {
		for {
			conn, err := listener.Accept()
			if err != nil {
				return // Listener closed
			}
			go handleConnection(conn)
		}
	}(ln)

	writer.Encode(ProtocolResponse{
		Status: "ok",
		Message: fmt.Sprintf("Server started on %s", addr),
	})
}

func handleStopServer(payload json.RawMessage, writer *json.Encoder) {
	var p StartServerPayload
	if err := json.Unmarshal(payload, &p); err != nil {
		sendError(writer, "Invalid payload for stop_server")
		return
	}

	addr := fmt.Sprintf(":%d", p.Port)

	state.Mutex.Lock()
	defer state.Mutex.Unlock()

	if ln, exists := state.Listeners[addr]; exists {
		ln.Close()
		delete(state.Listeners, addr)
		writer.Encode(ProtocolResponse{Status: "ok", Message: "Server stopped"})
	} else {
		sendError(writer, "Server not found")
	}
}

func handleStatus(writer *json.Encoder) {
	state.Mutex.Lock()
	defer state.Mutex.Unlock()

	active := []string{}
	for addr := range state.Listeners {
		active = append(active, addr)
	}

	writer.Encode(ProtocolResponse{
		Status: "ok",
		Data: map[string]interface{}{
			"active_servers": active,
			"goroutines":     1, // Placeholder
		},
	})
}

func handleConnection(conn net.Conn) {
	defer conn.Close()
	// Basic echo for now, or custom protocol logic
	// In a real scenario, this would handle high-speed data transfer
	buffer := make([]byte, 4096)
	conn.SetReadDeadline(time.Now().Add(30 * time.Second))
	
	for {
		n, err := conn.Read(buffer)
		if err != nil {
			return
		}
		// Echo back
		conn.Write(buffer[:n])
	}
}

func sendError(writer *json.Encoder, msg string) {
	writer.Encode(ProtocolResponse{Status: "error", Message: msg})
}
