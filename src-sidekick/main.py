import sys
import os
import psutil
import json
from PySide6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout, 
                             QLabel, QFrame, QSizePolicy, QPushButton)
from PySide6.QtCore import QTimer, Qt, QThread, Signal, QRectF
from PySide6.QtGui import QPainter, QColor, QPen, QFont, QDragEnterEvent, QDropEvent
from moviepy import VideoFileClip

# --- 🎨 Modern Circular Progress Bar ---
class CircularProgress(QWidget):
    def __init__(self, title, color_hex, parent=None):
        super().__init__(parent)
        self.value = 0
        self.title = title
        self.color = QColor(color_hex)
        self.setMinimumSize(100, 120)

    def set_value(self, val):
        self.value = val
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        
        # Boyutlar
        width = self.width()
        height = self.height()
        size = min(width, height) - 20
        rect = QRectF((width - size) / 2, (height - size) / 2 - 10, size, size)

        # 1. Arka Plan Çemberi
        painter.setPen(QPen(QColor("#333333"), 8, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        painter.drawEllipse(rect)

        # 2. İlerleme Yayı (Progress Arc)
        pen = QPen(self.color, 8, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap)
        painter.setPen(pen)
        # 360 * value / 100, -90 derece (saat 12 yönünden başla)
        span_angle = int(-360 * self.value / 100 * 16)
        painter.drawArc(rect, 90 * 16, span_angle)

        # 3. Ortadaki Yüzde Metni
        painter.setPen(QColor("#FFFFFF"))
        painter.setFont(QFont("Segoe UI", 12, QFont.Weight.Bold))
        painter.drawText(rect, Qt.AlignmentFlag.AlignCenter, f"{int(self.value)}%")

        # 4. Alt Başlık (CPU, RAM vb.)
        painter.setPen(QColor("#AAAAAA"))
        painter.setFont(QFont("Segoe UI", 10))
        text_rect = QRectF(0, height - 25, width, 20)
        painter.drawText(text_rect, Qt.AlignmentFlag.AlignCenter, self.title)
        
        painter.end()

# --- 🔄 Video Converter Thread ---
class ConverterThread(QThread):
    progress_updated = Signal(str) # Durum mesajı
    finished = Signal()

    def __init__(self, file_path):
        super().__init__()
        self.file_path = file_path

    def run(self):
        try:
            self.progress_updated.emit("Dönüştürme başlıyor...")
            
            # Basit bir mantık: mp4 ise mp3 yap, değilse mp4 yap
            file_name, ext = os.path.splitext(self.file_path)
            output_path = f"{file_name}_converted.mp3" if ext == ".mp4" else f"{file_name}_converted.mp4"
            
            clip = VideoFileClip(self.file_path)
            
            if ext == ".mp4":
                self.progress_updated.emit("MP4 -> MP3 Çıkarılıyor...")
                clip.audio.write_audiofile(output_path, logger=None) # Logger none to avoid console spam
            else:
                self.progress_updated.emit("Video formatına dönüştürülüyor...")
                clip.write_videofile(output_path, codec="libx264", logger=None)
            
            clip.close()
            self.progress_updated.emit(f"Tamamlandı: {os.path.basename(output_path)}")
        
        except Exception as e:
            self.progress_updated.emit(f"Hata: {str(e)}")
        finally:
            self.finished.emit()

# --- 👂 Stdin Listener (Rust Communication) ---
class StdinListener(QThread):
    def run(self):
        while True:
            try:
                line = sys.stdin.readline()
                if not line:
                    break
                line = line.strip()
                if not line:
                    continue
                
                try:
                    data = json.loads(line)
                    if data.get("type") == "omnibox_query":
                        self.handle_omnibox_query(data.get("query", ""))
                except json.JSONDecodeError:
                    pass
            except Exception:
                pass

    def handle_omnibox_query(self, payload):
        query = payload.get("query", "")
        context = payload.get("context", {})
        favorites = context.get("favorites", [])
        history = context.get("history", [])

        if not query:
            return

        suggestions = []
        query_lower = query.lower()
        
        # 1. Navigation (Smart detection)
        if "." in query and " " not in query:
             suggestions.append({
                "title": f"Go to {query}",
                "url": f"http://{query}" if not query.startswith("http") else query,
                "icon": "globe",
                "type": "navigation"
            })
        
        # 2. Real Favorites (from Context)
        for fav in favorites:
            if query_lower in fav.get("title", "").lower() or query_lower in fav.get("url", "").lower():
                 suggestions.append({
                    "title": fav.get("title", "Favorite"),
                    "url": fav.get("url", ""),
                    "icon": "star",
                    "type": "favorite"
                })

        # 3. Real History (from Context)
        # Limit history suggestions to 3 items to avoid clutter if many match
        hist_count = 0
        for item in history:
            if hist_count >= 3: break
            # Avoid duplicates with favorites or exact query
            if any(f.get("url") == item.get("url") for f in favorites):
                continue
            
            suggestions.append({
                "title": item.get("title", "History"),
                "url": item.get("url", ""),
                "icon": "clock",
                "type": "history"
            })
            hist_count += 1

        # 4. Search Engine
        suggestions.append({
            "title": f"Google Search: {query}",
            "url": f"https://www.google.com/search?q={query}",
            "icon": "search",
            "type": "search"
        })
        
        # 5. Internal Pages (Lumina)
        if "set" in query_lower:
            suggestions.append({
                "title": "Lumina Settings",
                "url": "lumina-app://settings",
                "icon": "settings",
                "type": "internal"
            })

        # 6. Math Calculation
        if any(op in query for op in ['+', '-', '*', '/']):
            try:
                # Basic safety: only allow numbers and operators
                if all(c in "0123456789+-*/. ()" for c in query):
                    result = eval(query)
                    suggestions.append({
                        "title": f"Calculation: {query} = {result}",
                        "url": f"https://www.google.com/search?q={query}",
                        "icon": "calculator",
                        "type": "calculation"
                    })
            except:
                pass

        # 7. Time/Date (Safkan Smart Info)
        if query_lower in ["time", "date", "saat", "tarih"]:
            from datetime import datetime
            now = datetime.now()
            suggestions.append({
                "title": f"Current Time: {now.strftime('%H:%M:%S')}",
                "url": "",
                "icon": "clock",
                "type": "info"
            })
            suggestions.append({
                "title": f"Current Date: {now.strftime('%Y-%m-%d')}",
                "url": "",
                "icon": "calendar",
                "type": "info"
            })

        # Output result
        response = {"suggestions": suggestions}
        print(f"OMNIBOX_RESULTS: {json.dumps(response)}", flush=True)

# --- 🚀 Ana Uygulama ---
class LuminaSidekick(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Lumina Sidekick")
        self.setFixedSize(500, 650)
        self.setStyleSheet("""
            QMainWindow { background-color: #121212; }
            QLabel { color: #E0E0E0; font-family: 'Segoe UI'; }
            QWidget#DropArea { 
                border: 2px dashed #444; 
                border-radius: 12px; 
                background-color: #1e1e1e; 
            }
            QWidget#DropArea:hover {
                border-color: #05B8CC;
                background-color: #252525;
            }
        """)

        # Main Widget
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        self.main_layout = QVBoxLayout(central_widget)
        self.main_layout.setSpacing(20)
        self.main_layout.setContentsMargins(30, 30, 30, 30)

        # 1. Başlık
        title = QLabel("LUMINA SIDEKICK")
        title.setStyleSheet("font-size: 18px; font-weight: bold; color: #05B8CC; letter-spacing: 2px;")
        title.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.main_layout.addWidget(title)

        # 2. Sistem İzleme (Dairesel Barlar)
        stats_container = QWidget()
        stats_layout = QHBoxLayout(stats_container)
        stats_layout.setSpacing(10)
        
        self.cpu_circle = CircularProgress("CPU", "#FF5252")
        self.ram_circle = CircularProgress("RAM", "#448AFF")
        self.disk_circle = CircularProgress("DISK", "#69F0AE")
        
        stats_layout.addWidget(self.cpu_circle)
        stats_layout.addWidget(self.ram_circle)
        stats_layout.addWidget(self.disk_circle)
        
        self.main_layout.addWidget(stats_container)

        # 3. Converter Drop Area
        self.drop_area = QWidget()
        self.drop_area.setObjectName("DropArea")
        drop_layout = QVBoxLayout(self.drop_area)
        
        drop_label = QLabel("Drag & Drop Video File")
        drop_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        drop_label.setStyleSheet("color: #aaa; font-size: 16px; border: none; background: transparent;")
        drop_layout.addWidget(drop_label)
        
        # Restore Drag & Drop
        self.drop_area.setAcceptDrops(True)
        # We need to override the event handlers for the widget, but since we can't easily subclass here without more code,
        # let's just use the main window's events which are redirected or keep the previous logic.
        # Actually, let's just re-add the assignment if the methods exist on the main window.
        # But wait, assigning methods to an instance like this in Python works but is hacky.
        # Better: Just let the Main Window handle drops if setAcceptDrops is True on it.
        
        self.main_layout.addWidget(self.drop_area)

        # 4. Lua Bridge Test (The Bridge)
        self.lua_btn = QPushButton("FIRE LUA BRIDGE")
        self.lua_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self.lua_btn.setStyleSheet("""
            QPushButton {
                background-color: #7C4DFF; color: white; border-radius: 8px;
                padding: 12px; font-weight: bold; font-size: 14px; letter-spacing: 1px;
            }
            QPushButton:hover { background-color: #651FFF; }
        """)
        self.lua_btn.clicked.connect(self.fire_lua_bridge)
        self.main_layout.addWidget(self.lua_btn)

        # Timer for system stats
        self.timer = QTimer()
        self.timer.timeout.connect(self.update_stats)
        self.timer.start(1000)

        self.setAcceptDrops(True)

        # Start Stdin Listener
        self.stdin_listener = StdinListener()
        self.stdin_listener.start()

    def fire_lua_bridge(self):
        # Sends a Lua script to Rust via stdout
        # Rust captures this, executes Lua, and updates the frontend
        print('LUA: return "Bridge Successful: " .. os.date("%Y-%m-%d %H:%M:%S")', flush=True)

    def update_stats(self):
        self.cpu_circle.set_value(psutil.cpu_percent())
        self.ram_circle.set_value(psutil.virtual_memory().percent)
        self.disk_circle.set_value(psutil.disk_usage('/').percent)

    def dragEnterEvent(self, event: QDragEnterEvent):
        if event.mimeData().hasUrls():
            event.accept()
            self.drop_area.setStyleSheet("border: 2px dashed #05B8CC; border-radius: 12px; background-color: #252525; color: #FFF;")
        else:
            event.ignore()

    def dropEvent(self, event: QDropEvent):
        self.drop_area.setStyleSheet("border: 2px dashed #444; border-radius: 12px; background-color: #1e1e1e; color: #888;")
        files = [u.toLocalFile() for u in event.mimeData().urls()]
        
        if files:
            file_path = files[0]
            self.start_conversion(file_path)

    def start_conversion(self, file_path):
        self.status_label.setStyleSheet("color: #05B8CC;")
        self.worker = ConverterThread(file_path)
        self.worker.progress_updated.connect(self.status_label.setText)
        self.worker.finished.connect(self.on_conversion_finished)
        self.worker.start()

    def on_conversion_finished(self):
        self.status_label.setStyleSheet("color: #69F0AE;")
        # 3 saniye sonra "Hazır" yazısına dön
        QTimer.singleShot(3000, lambda: self.status_label.setText("Hazır"))
        QTimer.singleShot(3000, lambda: self.status_label.setStyleSheet("color: #666;"))

if __name__ == "__main__":
    app = QApplication(sys.argv)
    window = LuminaSidekick()
    window.show()
    sys.exit(app.exec())
