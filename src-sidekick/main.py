import sys
import os
import psutil
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, QHBoxLayout, 
                             QLabel, QFrame, QSizePolicy)
from PyQt6.QtCore import QTimer, Qt, QThread, pyqtSignal, QRectF
from PyQt6.QtGui import QPainter, QColor, QPen, QFont, QDragEnterEvent, QDropEvent
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
    progress_updated = pyqtSignal(str) # Durum mesajı
    finished = pyqtSignal()

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

        # 3. Converter Alanı (Drag & Drop)
        self.main_layout.addSpacing(10)
        self.main_layout.addWidget(QLabel("MEDYA DÖNÜŞTÜRÜCÜ"))
        
        self.drop_area = QLabel("\n📂\n\nDosyayı Buraya Sürükleyin\n(.mp4, .avi, .mov)")
        self.drop_area.setObjectName("DropArea")
        self.drop_area.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.drop_area.setMinimumHeight(180)
        self.drop_area.setStyleSheet("font-size: 14px; color: #888;")
        self.drop_area.setAcceptDrops(True)
        
        # Drag & Drop olaylarını bağla
        self.drop_area.dragEnterEvent = self.dragEnterEvent
        self.drop_area.dropEvent = self.dropEvent
        
        self.main_layout.addWidget(self.drop_area)
        
        # Durum Çubuğu (Dönüştürme bilgisi için)
        self.status_label = QLabel("Hazır")
        self.status_label.setStyleSheet("color: #666; font-size: 12px;")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.main_layout.addWidget(self.status_label)

        # Timer (Sistem istatistikleri için)
        self.timer = QTimer()
        self.timer.timeout.connect(self.update_stats)
        self.timer.start(1000) # 1 saniye

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
